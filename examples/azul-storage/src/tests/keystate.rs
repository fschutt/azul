//! Key permits (ARCH29 C7) through the functions the flows call: the sentences beside the flows that say
//! them, the registry of what a drive key touches, and the flows that ask the key permit - a rotation, a
//! re-encryption, a recovery, the recovery wraps, the restore - run over real drives, stopped in each of
//! their windows. The truth table of the judge, the observation of the state and the cells that depend on
//! what was read are `keystate_table.rs`, a child of the module: the state and the judge are private to
//! it. Here a Permit is made by `ask` on a fixture - a drive set up on a device - and never from a state
//! made up for the test.

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
        device::{
            self, add_recovery_code, recover, restore_permit, setup_new_drive, wraps_permit,
            MAX_EXTRA_RECOVERY_CODES,
        },
        keys::{load_key_chain, RecoveryKdf},
        keystate::{
            ask, forget, owner, Change, Done, Erase, Evidence, Forget, Journal, Op, Owner, Permit,
            Read, RecoveryRecord, Refusal, Reseal, TokenFacts, VaultFacts, Verdict, Way, CONSUMERS,
            MEMBERS,
        },
        DriveKey,
    },
    encrypted::{
        judge as enc_judge, open_encrypted, IndexProvider, MemoryIndex, NameIndex, Recorded,
    },
    footprint::KeyEntry,
    keyring::{KeyringError, KeyringStore, MemoryKeyring},
    meta::{IndexProfile, MemoryBucket, MetaIndexProvider},
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

/// X-DELTA-STRUCT-KEYSTATE-10: the sentence of a drive whose keys open nothing, entered through the flow - not
/// read from two accessors of one constant. A device that kept the key from before another device changed the
/// keys (the keyring's drive key is K1, the drive's index is K2's) is refused by `rotation::ask`, before the
/// account's lockdown, in the words the window shows: what happened, that nothing was changed, and the ways
/// back in - a join code from the device that changed them, or the recovery code. One sentence with single
/// spaces, and none the window ends itself with a full stop.
#[test]
fn x_delta_struct_keystate_10_a_device_whose_key_opens_nothing_is_refused_in_the_words_the_window_shows(
) {
    let bucket = Arc::new(MemoryBucket::new());
    let owner = MemoryKeyring::new();
    let (old, _code) = setup_new_drive(bucket.as_ref(), &owner, DRIVE, cheap()).unwrap();
    let provider = MetaIndexProvider::new(IndexProfile::in_memory("Laptop"));
    let plain: Arc<dyn Drive> = bucket.clone();
    open_encrypted(plain.clone(), &owner, DRIVE, &provider)
        .unwrap()
        .put("notes/plan.txt", b"the plan")
        .unwrap();
    // a second device that holds the key from before the owner's key change
    let behind = MemoryKeyring::new();
    device::store_drive_key(&behind, DRIVE, &old).unwrap();
    rotate(plain, &owner, DRIVE, &provider, cheap()).unwrap();

    let refused = rotation::ask(bucket.as_ref(), &behind, DRIVE)
        .err()
        .expect("a key that opens nothing is no key to change");
    let DriveError::Denied { message } = refused else {
        panic!("{refused:?}");
    };
    for words in [
        "none of the drive keys this device holds",
        "Nothing was changed here",
        "a code from that device",
        "recovery code",
    ] {
        assert!(message.contains(words), "{words:?}: {message}");
    }
    assert!(
        !message.contains("  "),
        "single spaces (d3607fed5): {message}"
    );
    assert!(
        !message.ends_with('.'),
        "the window ends it itself: {message}"
    );
    // and it is what the table says of the state: torn, with the recovery as the way back
    assert_eq!(
        message,
        Refusal::OpensNothing.sentence(DRIVE),
        "the flow and the table say it in the same words"
    );
}

/// X-DELTA-STRUCT-KEYSTATE-10: the sentence of a locked drive, entered through the flows: a device that holds no
/// key of an encrypted drive is told the same by the open and by a key change - `Locked`, in the table's words.
#[test]
fn x_delta_struct_keystate_10_a_device_with_no_key_is_told_the_same_by_an_open_and_by_a_key_change()
{
    let bucket = Arc::new(MemBucket::new());
    setup_new_drive(bucket.as_ref(), &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let stranger = MemoryKeyring::new();
    let plain: Arc<dyn Drive> = bucket.clone();
    let opened = open_encrypted(plain, &stranger, DRIVE, &Provider::new())
        .expect_err("no key, no drive");
    let asked = rotation::ask(bucket.as_ref(), &stranger, DRIVE)
        .err()
        .expect("no key, no key change");
    assert!(matches!(opened, DriveError::Locked { .. }), "{opened:?}");
    assert!(matches!(asked, DriveError::Locked { .. }), "{asked:?}");
    assert_eq!(opened.to_string(), asked.to_string());
    assert_eq!(asked.to_string(), Refusal::Locked.sentence(DRIVE));

    // and a device that has no key of a drive nobody set up, or whose own setup it did not finish, is told
    // that in the table's words - not "finish it on the device that started it", of a key change nobody began
    let nothing = MemBucket::new();
    let said = |error: DriveError| match error {
        DriveError::Denied { message } => message,
        other => panic!("{other:?}"),
    };
    let new_drive = rotation::ask(&nothing, &MemoryKeyring::new(), DRIVE)
        .err()
        .expect("no key, no key change");
    assert_eq!(said(new_drive), Refusal::NoKey.sentence(DRIVE));
    let half_set_up = MemoryKeyring::new();
    device::plan_drive_key(&half_set_up, DRIVE, &DriveKey::generate().unwrap()).unwrap();
    let unfinished = rotation::ask(&nothing, &half_set_up, DRIVE)
        .err()
        .expect("a setup that is not finished");
    assert_eq!(said(unfinished), Refusal::SetupUnfinished.sentence(DRIVE));
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

/// The design lists ten things a drive key touches. The registry has twelve: the index is two of them
/// because a rotation seals the index anew (a step an index that cannot be rekeyed stops, before anything
/// changed) and wraps the entries' file keys anew (a step of its own in the journal) - two functions the
/// rotation runs with a checkpoint between, and a stop between them is a state the next call goes on from -
/// and the id of the key the apps record (X-DELTA-STRUCT-KEYSTATE-9) is listed, though nothing of it is
/// sealed under the key.
#[test]
fn a_drive_key_touches_twelve_things_the_designs_ten_the_index_in_two_steps_and_the_recorded_key_id(
) {
    let names: Vec<&str> = CONSUMERS.iter().map(|c| c.name).collect();
    assert_eq!(
        names,
        [
            "INDEX",
            "FILE_KEYS",
            "MEMBERS",
            "RECOVERY_WRAP",
            "KITS",
            "SHARES",
            "PERIOD_VAULT",
            "DROPS",
            "REENCRYPT_STATE",
            "CHAIN",
            "DEVICE_KEY",
            "RECORDED_KEY_ID"
        ]
    );
    let unique: BTreeSet<&str> = names.iter().copied().collect();
    assert_eq!(unique.len(), 12);
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

/// What a rotation reseals: seven consumers do it here, one in another consumer's pass, three are not
/// sealed under the drive key or are resealed by what the app does.
#[test]
fn seven_consumers_reseal_here_and_four_forget_a_keyring_entry() {
    let resealed: Vec<&str> = CONSUMERS
        .iter()
        .filter(|c| matches!(c.reseal, Reseal::Fn(_)))
        .map(|c| c.name)
        .collect();
    assert_eq!(
        resealed,
        [
            "INDEX",
            "FILE_KEYS",
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

/// A recovery record in a test: a code typed back right ten seconds before `NOW`, and registered.
struct CheckedCode;

impl RecoveryRecord for CheckedCode {
    fn code_checked(&self) -> Read<u64> {
        Read::Found(NOW - 10)
    }
    fn code_registered(&self) -> Read<bool> {
        Read::Found(true)
    }
}

/// `e` as a removal of a drive with a perfect proof is asked: the recovery code was checked and
/// registered, and the drive has no vault of its own to lose months from.
fn with_a_good_proof(mut e: Evidence<'static>) -> Evidence<'static> {
    e.token = TokenFacts::of(&CheckedCode);
    e.vault = VaultFacts::no_vault();
    e
}

/// The permit of `op` on a drive set up on a device: the key permit asked on what the keyring and the
/// bucket of that drive say, with a recovery code that was checked - never a state made up for it.
fn permit_of(op: Op) -> Permit {
    let (bucket, keyring) = a_drive_set_up();
    let mut e = with_a_good_proof(gathered(&bucket, &keyring));
    e.bucket.extra_codes = Read::Found(0);
    match ask(op, &e) {
        Verdict::Permit(permit) => permit,
        Verdict::Refuse(reason) => panic!("{op:?}: {reason}"),
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
    let enrol = permit_of(Op::Enroll);
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
    let removal = permit_of(Op::Remove);
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

/// `forget` is the one way a removal takes an entry of the drive out of the keyring, for the permit of a
/// removal of this drive: every kind of entry of [`KeyEntry::ALL`] goes through it - the keys, the
/// consumers' entries by their own function - and no entry of another drive does; for a permit of another
/// operation or drive nothing goes.
#[test]
fn the_registry_forgets_every_kind_of_entry_for_a_removal_permit_and_for_no_other() {
    let keyring = MemoryKeyring::new();
    for drive in [DRIVE, "d_other"] {
        for entry in KeyEntry::ALL {
            keyring.set(&entry.name(drive), "a secret text").unwrap();
        }
    }
    let erase = Erase {
        drive: DRIVE,
        keyring: &keyring,
    };
    let enrol = permit_of(Op::Enroll);
    let removal = permit_of(Op::Remove);

    for entry in KeyEntry::ALL {
        assert!(
            matches!(
                forget(&enrol, &erase, entry),
                Err(DriveError::Denied { .. })
            ),
            "{entry:?}: a permit of another operation"
        );
        assert!(
            keyring.get(&entry.name(DRIVE)).unwrap().is_some(),
            "{entry:?}"
        );
    }
    let other_drive = Erase {
        drive: "d_other",
        keyring: &keyring,
    };
    assert!(matches!(
        forget(&removal, &other_drive, KeyEntry::DriveKey),
        Err(DriveError::Denied { .. })
    ));

    for entry in KeyEntry::ALL {
        assert!(forget(&removal, &erase, entry).is_ok(), "{entry:?}");
        assert!(
            keyring.get(&entry.name(DRIVE)).unwrap().is_none(),
            "{entry:?} is still there"
        );
        assert!(
            keyring.get(&entry.name("d_other")).unwrap().is_some(),
            "{entry:?} of another drive"
        );
    }
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
    let rotate = permit_of(Op::Rotate);
    let enrol = permit_of(Op::Enroll);

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

    // the index: the provider seals it under the new key; then the entries' file keys, of which the empty
    // index has none to re-wrap
    let index = CONSUMERS.iter().find(|c| c.name == "INDEX").unwrap();
    let Reseal::Fn(reseal_index) = index.reseal else {
        panic!("INDEX is resealed here");
    };
    assert!(matches!(
        reseal_index(&enrol, &change),
        Err(DriveError::Denied { .. })
    ));
    assert!(matches!(reseal_index(&rotate, &change), Ok(Done::Nothing)));
    let file_keys = CONSUMERS.iter().find(|c| c.name == "FILE_KEYS").unwrap();
    let Reseal::Fn(reseal_file_keys) = file_keys.reseal else {
        panic!("FILE_KEYS is resealed here");
    };
    assert!(matches!(
        reseal_file_keys(&enrol, &change),
        Err(DriveError::Denied { .. })
    ));
    assert!(matches!(
        reseal_file_keys(&rotate, &change),
        Ok(Done::Count(0))
    ));

    // what a rotation calls is `reseal_for`: the function above for a consumer that has one, and for one
    // that is resealed in another's pass or is not sealed under the drive key nothing - but the permit is
    // looked at all the same
    for name in ["KITS", "PERIOD_VAULT", "REENCRYPT_STATE", "DEVICE_KEY"] {
        let consumer = CONSUMERS.iter().find(|c| c.name == name).unwrap();
        assert!(
            matches!(
                consumer.reseal_for(&enrol, &change),
                Err(DriveError::Denied { .. })
            ),
            "{name}: a permit of another operation"
        );
        assert!(
            matches!(consumer.reseal_for(&rotate, &change), Ok(Done::Nothing)),
            "{name}"
        );
    }

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

/// A key change that stopped at ANY of its writes - before the new key is the drive key, after it, with the
/// journal or without - refuses a code added, replaced or taken out, an invite and a member's removal in the key
/// change's own words (the recovery wrap may change; the key change is not finished), never in "was not looked
/// at": these flows ask without a bucket, so the journal is not read, and the entries of the keyring already say
/// a key change is not over. The window after the Members step - the drive key IS the new key - is among them.
#[test]
fn a_key_change_stopped_at_any_write_refuses_the_wraps_and_the_invite_in_its_own_words() {
    let mut members_window = 0;
    let mut stopped = 0;
    for nth in 1..=40 {
        let (bucket, keyring, _code) = a_drive_with_its_code();
        let failing: Arc<dyn Drive> = Arc::new(NthWrite::new(bucket.clone(), nth));
        if rotate(failing, &keyring, DRIVE, &Provider::new(), cheap()).is_ok() {
            break;
        }
        stopped += 1;
        let (_, next) = device::rotation_key_entries(DRIVE);
        let new_key = device::load_key_at(&keyring, &next).unwrap();
        if new_key.is_some() && new_key == device::load_drive_key(&keyring, DRIVE).unwrap() {
            members_window += 1;
        }
        for op in [Op::RemoveCode, Op::NewCode] {
            let refused = wraps_permit(op, &keyring, DRIVE);
            assert!(
                matches!(&refused, Err(DriveError::Denied { message }) if message.contains("recovery wrap")),
                "write {nth}, {op:?}: {:?}",
                refused.as_ref().err()
            );
        }
        for op in [Op::Invite, Op::RemoveMember] {
            let refused = wraps_permit(op, &keyring, DRIVE);
            assert!(
                matches!(&refused, Err(DriveError::Denied { message }) if message.contains("not finished")),
                "write {nth}, {op:?}: {:?}",
                refused.as_ref().err()
            );
        }
    }
    assert!(
        stopped >= 5,
        "the key change has windows to stop in: {stopped}"
    );
    assert!(
        members_window >= 1,
        "one of them is the window after the Members step"
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
    e.keyring.keeps_keys = device::keeps_keys_fact(keyring);
    e.bucket.index_opens = Read::Found(true);
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

/// What the key permit says of the device `e` gathers, in one word: a setup it permits (`new`, `finish`),
/// else the way a rotation would go on (`rotate`, `resume`, `adopt`), else why it is refused (`locked`,
/// `torn`, `unknown`, `planned`).
fn says(e: &Evidence<'_>) -> &'static str {
    if let Verdict::Permit(permit) = ask(Op::Setup, e) {
        return if permit.way() == Way::Finish {
            "finish"
        } else {
            "new"
        };
    }
    match ask(Op::Rotate, e) {
        Verdict::Permit(permit) => match permit.way() {
            Way::New => "rotate",
            Way::Resume => "resume",
            Way::Adopt => "adopt",
            Way::Finish | Way::Plain => "other",
        },
        Verdict::Refuse(reason) => match reason.kind {
            Refusal::Locked => "locked",
            Refusal::OldKeyGone | Refusal::OpensNothing => "torn",
            Refusal::Unreadable { .. } | Refusal::NotAsked(_) => "unknown",
            Refusal::SetupUnfinished => "planned",
            _ => "refused",
        },
    }
}

/// The key permit agrees with the local observers: over seven real configurations of a drive it says
/// what each of them says (`rotation::unfinished`, `resumable`, `start`, `lockdown_needed`,
/// `device::setup_pending`, `encrypted::judge`), asked through `ask` - the state it observes is private
/// to the module - and the table's own rows are `keystate_table.rs`.
#[test]
fn the_key_permit_says_what_each_local_observer_says_over_real_configurations() {
    let drive_key_of = |keyring: &dyn KeyringStore| device::load_drive_key(keyring, DRIVE).unwrap();

    // Fresh: nothing anywhere.
    let bucket = MemBucket::new();
    let keyring = MemoryKeyring::new();
    let e = gathered(&bucket, &keyring);
    assert_eq!(says(&e), "new");
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
    assert_eq!(says(&e), "finish");
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
    assert_eq!(says(&e), "rotate");
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
    assert_eq!(says(&e), "locked");
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
    assert_eq!(says(&e), "resume");
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
    assert_eq!(says(&e), "adopt");
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
    assert_eq!(says(&e), "adopt", "a planted journal is not this device's");
    assert!(lockdown_needed(bucket.as_ref(), &keyring, DRIVE).unwrap());
    bucket.set(JOURNAL_FILE, journal);

    // Torn: the new key is kept and the drive key it replaces is gone, and the journal is not ours.
    let drive_key_entry = device::drive_key_entry(DRIVE);
    let held = drive_key_of(&keyring).unwrap();
    bucket.delete(JOURNAL_FILE).unwrap();
    keyring.delete(&drive_key_entry).unwrap();
    let e = gathered(&bucket, &keyring);
    assert_eq!(says(&e), "torn");
    assert!(
        rotation::unfinished(&keyring, DRIVE).unwrap(),
        "the drive key is not the new key"
    );
    assert!(rotation::start(bucket.as_ref(), &keyring, DRIVE).is_err());
    assert!(lockdown_needed(bucket.as_ref(), &keyring, DRIVE).is_err());
    device::store_drive_key(&keyring, DRIVE, &held).unwrap();

    // Unknown: a keyring that cannot be read says nothing, and the observers fail with it.
    let e = gathered(&bucket, &Unreadable);
    assert_eq!(says(&e), "unknown");
    assert!(rotation::unfinished(&Unreadable, DRIVE).is_err());
    assert!(device::setup_pending(&Unreadable, DRIVE).is_err());
    assert!(rotation::start(bucket.as_ref(), &Unreadable, DRIVE).is_err());
}

/// X-DELTA-STRUCT-KEYSTATE-9: `encrypted::judge` - the EncState gate every app opens a drive by - and the key
/// permit read the same three things of a device: whether the keyring holds a drive key, whether a key is
/// planned for a setup, whether the bucket lists key files. They agree on EVERY combination of them (the test
/// above is seven picked ones): a drive with a key is encrypted and the permit lets a key change through (a
/// planned key that IS the drive key is a setup that stopped before its last step: the same call finishes it);
/// with no key, a planned key is a setup to finish whatever the bucket lists, key files make a locked drive, and
/// neither is a new one. A change to either reading that the other does not follow fails here.
#[test]
fn x_delta_struct_keystate_9_the_key_permit_and_the_enc_state_gate_agree_on_every_combination_they_both_read(
) {
    #[derive(Clone, Copy, Debug)]
    enum Planned {
        No,
        Another,
        TheDriveKey,
    }
    for holds_key in [false, true] {
        for planned in [Planned::No, Planned::Another, Planned::TheDriveKey] {
            for listed in [false, true] {
                let combination = format!("key {holds_key}, planned {planned:?}, listed {listed}");
                // a bucket that lists key files: a drive another device set up
                let bucket = MemBucket::new();
                let theirs = DriveKey::generate().unwrap();
                if listed {
                    setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
                }
                let keyring = MemoryKeyring::new();
                if holds_key {
                    device::store_drive_key(&keyring, DRIVE, &theirs).unwrap();
                }
                match planned {
                    Planned::No => {}
                    Planned::Another => {
                        device::plan_drive_key(&keyring, DRIVE, &DriveKey::generate().unwrap())
                            .unwrap();
                    }
                    Planned::TheDriveKey => {
                        // only a drive key can be the planned one
                        if !holds_key {
                            continue;
                        }
                        device::plan_drive_key(&keyring, DRIVE, &theirs).unwrap();
                    }
                }
                let gate = enc_judge(&bucket, &keyring, DRIVE, Recorded::default()).unwrap();
                let permit = says(&gathered(&bucket, &keyring));
                match (holds_key, planned) {
                    (true, Planned::TheDriveKey) => {
                        assert!(
                            matches!(
                                gate,
                                EncState::Encrypted {
                                    via: Via::KeyInKeyring,
                                    ..
                                }
                            ),
                            "{combination}: {gate:?}"
                        );
                        assert_eq!(permit, "finish", "{combination}");
                    }
                    (true, _) => {
                        assert!(
                            matches!(
                                gate,
                                EncState::Encrypted {
                                    via: Via::KeyInKeyring,
                                    ..
                                }
                            ),
                            "{combination}: {gate:?}"
                        );
                        assert_eq!(permit, "rotate", "{combination}");
                    }
                    (false, Planned::Another) => {
                        assert_eq!(gate, EncState::SetupPending, "{combination}");
                        assert_eq!(permit, "finish", "{combination}");
                    }
                    (false, _) if listed => {
                        assert!(
                            matches!(
                                gate,
                                EncState::Encrypted {
                                    via: Via::KeyFilesListed,
                                    ..
                                }
                            ),
                            "{combination}: {gate:?}"
                        );
                        assert_eq!(permit, "locked", "{combination}");
                    }
                    (false, _) => {
                        assert_eq!(
                            gate,
                            EncState::Unencrypted(Unencrypted::New),
                            "{combination}"
                        );
                        assert_eq!(permit, "new", "{combination}");
                    }
                }
            }
        }
    }

    // The window after the Members step: the drive key IS the new key and the key it replaced is still kept - a
    // key change that is not over, which the gate cannot see (a drive key in the keyring is an open drive to it).
    // The one place the permit reads more than the gate: no removal goes through.
    let bucket = MemBucket::new();
    setup_new_drive(&bucket, &MemoryKeyring::new(), DRIVE, cheap()).unwrap();
    let keyring = MemoryKeyring::new();
    let (k1, k2) = (DriveKey::generate().unwrap(), DriveKey::generate().unwrap());
    device::store_drive_key(&keyring, DRIVE, &k2).unwrap();
    let (previous, next) = device::rotation_key_entries(DRIVE);
    device::store_key_at(&keyring, &previous, &k1).unwrap();
    device::store_key_at(&keyring, &next, &k2).unwrap();
    let gate = enc_judge(&bucket, &keyring, DRIVE, Recorded::default()).unwrap();
    assert!(
        matches!(
            gate,
            EncState::Encrypted {
                via: Via::KeyInKeyring,
                ..
            }
        ),
        "after the Members step: {gate:?}"
    );
    match ask(Op::Remove, &with_a_good_proof(gathered(&bucket, &keyring))) {
        Verdict::Refuse(reason) => assert_eq!(reason.kind, Refusal::KeyChangeUnfinished),
        Verdict::Permit(_) => panic!("a removal went through a key change that is not over"),
    }
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
    ask(Op::Remove, &e)
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

// ==== C7 step 3: "re-encrypt everything" takes the permit of its own operation ====

/// A drive set up on a device, opened as the apps open it.
fn an_open_drive() -> (
    Arc<MemBucket>,
    MemoryKeyring,
    crate::encrypted::EncryptedDrive<Arc<dyn Drive>>,
) {
    let (bucket, keyring) = a_drive_set_up();
    let plain: Arc<dyn Drive> = bucket.clone();
    let drive = crate::encrypted::open_encrypted(plain, &keyring, DRIVE, &Provider::new()).unwrap();
    drive.put("notes/plan.txt", b"the plan").unwrap();
    (bucket, keyring, drive)
}

/// A pass asked in an Active drive runs; asked while a key change of this device is not over it is refused
/// (the files would be written under the key the change replaces, and the guard of the old flow looked at
/// nothing: the shadow's `known` finding of X-KEYSTATE-2) and nothing is written.
#[test]
fn a_reencryption_is_permitted_in_an_active_drive_and_refused_while_a_key_change_is_unfinished() {
    let (bucket, keyring, drive) = an_open_drive();
    let permit = rotation::ask_reencrypt(&keyring, DRIVE, &drive).unwrap();
    assert_eq!(permit.op(), Op::Reencrypt);
    let mut state = rotation::ReencryptState::new(crate::time::now_unix() + 10);
    assert!(
        rotation::reencrypt_pass(&permit, &drive, &mut state, &mut |_| Ok(()), &|| false).unwrap()
    );

    // A key change of this device stops after its journal went in.
    let failing: Arc<dyn Drive> = Arc::new(NthWrite::new(bucket.clone(), 3));
    assert!(rotate(failing, &keyring, DRIVE, &Provider::new(), cheap()).is_err());
    assert!(rotation::unfinished(&keyring, DRIVE).unwrap());
    let objects = bucket.objects();
    let refused = rotation::ask_reencrypt(&keyring, DRIVE, &drive);
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("not finished")),
        "{:?}",
        refused.as_ref().err()
    );
    assert_eq!(bucket.objects(), objects, "nothing was written");
}

/// A pass takes a permit of ITS operation, asked for the drive key the drive is opened with: the permit of
/// a rotation, and the permit of another drive's key, change nothing.
#[test]
fn a_reencryption_takes_a_permit_of_its_own_operation_and_drive_key_and_changes_nothing_for_another(
) {
    let (bucket, keyring, drive) = an_open_drive();
    let objects = bucket.objects();
    let mut state = rotation::ReencryptState::new(crate::time::now_unix() + 10);

    let rotation = rotation::ask(bucket.as_ref(), &keyring, DRIVE).unwrap();
    let refused = rotation::reencrypt_pass(
        rotation.permit(),
        &drive,
        &mut state,
        &mut |_| Ok(()),
        &|| false,
    );
    assert!(
        matches!(&refused, Err(DriveError::Denied { .. })),
        "{refused:?}"
    );

    // the permit asked for ANOTHER drive's key
    let (other_bucket, other_keyring, other_drive) = {
        let bucket = Arc::new(MemBucket::new());
        let keyring = MemoryKeyring::new();
        setup_new_drive(bucket.as_ref(), &keyring, "d_other", cheap()).unwrap();
        let plain: Arc<dyn Drive> = bucket.clone();
        let drive =
            crate::encrypted::open_encrypted(plain, &keyring, "d_other", &Provider::new()).unwrap();
        (bucket, keyring, drive)
    };
    let others = rotation::ask_reencrypt(&other_keyring, "d_other", &other_drive).unwrap();
    let refused = rotation::reencrypt_pass(&others, &drive, &mut state, &mut |_| Ok(()), &|| false);
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("another drive key")),
        "{refused:?}"
    );
    assert_eq!(bucket.objects(), objects, "nothing was written");
    drop(other_bucket);
}

// ==== C7 step 3: recover, join, the recovery wraps and the restore ask the key permit ====

/// A drive set up on one device (the code of its recovery wrap is kept for the tests that recover).
fn a_drive_with_its_code() -> (
    Arc<MemBucket>,
    MemoryKeyring,
    crate::crypto::keys::RecoveryCode,
) {
    let bucket = Arc::new(MemBucket::new());
    let keyring = MemoryKeyring::new();
    let (_key, code) = setup_new_drive(bucket.as_ref(), &keyring, DRIVE, cheap()).unwrap();
    (bucket, keyring, code)
}

/// X-KEYSTATE-5: a recovery - and a join with a code's invite - wrote the key it brought over a device
/// whose own setup was not finished (its planned key and the recovered one would pin two drives' keys), and
/// over a device whose key change was not over; the judge's Recover cell refuses both, before the
/// bucket is read and the code's Argon2id spent, and nothing is written.
#[test]
fn x_keystate_5_a_recovery_over_a_planned_or_unfinished_device_is_refused_before_anything_is_written(
) {
    // a device with a planned key of its own: another drive's setup that did not finish
    let (bucket, _owner, code) = a_drive_with_its_code();
    let planned_device = MemoryKeyring::new();
    let planned = crate::crypto::DriveKey::generate().unwrap();
    device::plan_drive_key(&planned_device, DRIVE, &planned).unwrap();
    let objects = bucket.objects();
    let refused = recover(bucket.as_ref(), &planned_device, DRIVE, &code);
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("did not finish")),
        "{:?}",
        refused.as_ref().err()
    );
    assert_eq!(bucket.objects(), objects, "no member wrap was written");
    assert!(device::load_drive_key(&planned_device, DRIVE)
        .unwrap()
        .is_none());
    assert_eq!(
        device::load_key_at(&planned_device, &device::planned_key_entry(DRIVE)).unwrap(),
        Some(planned),
        "the planned key is still there"
    );

    // the device whose key change stopped: the same recovery is refused too
    let (bucket, keyring, code) = a_drive_with_its_code();
    let failing: Arc<dyn Drive> = Arc::new(NthWrite::new(bucket.clone(), 3));
    assert!(rotate(failing, &keyring, DRIVE, &Provider::new(), cheap()).is_err());
    let objects = bucket.objects();
    let held = device::load_drive_key(&keyring, DRIVE).unwrap();
    let refused = recover(bucket.as_ref(), &keyring, DRIVE, &code);
    assert!(
        matches!(&refused, Err(DriveError::Denied { message }) if message.contains("not finished")),
        "{:?}",
        refused.as_ref().err()
    );
    assert_eq!(bucket.objects(), objects);
    assert_eq!(device::load_drive_key(&keyring, DRIVE).unwrap(), held);

    // a device with no key at all takes the key from the code, as always
    let (bucket, _owner, code) = a_drive_with_its_code();
    let stranger = MemoryKeyring::new();
    assert!(recover(bucket.as_ref(), &stranger, DRIVE, &code).is_ok());
    assert!(device::load_drive_key(&stranger, DRIVE).unwrap().is_some());
}

/// X-KEYSTATE-2: a code added, replaced or taken out while a key change is not over would open, or close, a
/// way that the new key's code does not (the key change replaces the recovery wrap again): refused, nothing
/// written. The restore as of a time and an invite follow the same table.
#[test]
fn x_keystate_2_the_recovery_wraps_an_invite_and_a_restore_wait_for_an_unfinished_key_change() {
    let (bucket, keyring, _code) = a_drive_with_its_code();
    // Active: all of them are permitted
    for op in [Op::RemoveCode, Op::NewCode, Op::Invite, Op::RemoveMember] {
        assert!(wraps_permit(op, &keyring, DRIVE).is_ok(), "{op:?}");
    }
    assert!(restore_permit(&keyring, DRIVE, true).is_ok());
    // a device with no key: a drive that is not encrypted has no history to restore from, one that is
    // encrypted is locked here
    let stranger = MemoryKeyring::new();
    assert!(
        matches!(restore_permit(&stranger, DRIVE, false), Err(DriveError::Denied { message }) if message.contains("not encrypted")),
    );
    assert!(matches!(
        restore_permit(&stranger, DRIVE, true),
        Err(DriveError::Locked { .. })
    ));

    // a key change that stopped
    let failing: Arc<dyn Drive> = Arc::new(NthWrite::new(bucket.clone(), 3));
    assert!(rotate(failing, &keyring, DRIVE, &Provider::new(), cheap()).is_err());
    let objects = bucket.objects();
    for op in [Op::RemoveCode, Op::NewCode] {
        let refused = wraps_permit(op, &keyring, DRIVE);
        assert!(
            matches!(&refused, Err(DriveError::Denied { message }) if message.contains("recovery wrap")),
            "{op:?}: {:?}",
            refused.as_ref().err()
        );
    }
    for op in [Op::Invite, Op::RemoveMember] {
        let refused = wraps_permit(op, &keyring, DRIVE);
        assert!(
            matches!(&refused, Err(DriveError::Denied { message }) if message.contains("not finished")),
            "{op:?}: {:?}",
            refused.as_ref().err()
        );
    }
    let added = add_recovery_code(bucket.as_ref(), &keyring, DRIVE, cheap());
    assert!(
        matches!(&added, Err(DriveError::Denied { message }) if message.contains("recovery wrap")),
        "{:?}",
        added.as_ref().err()
    );
    assert!(matches!(
        restore_permit(&keyring, DRIVE, true),
        Err(DriveError::Denied { .. })
    ));
    assert_eq!(bucket.objects(), objects, "no wrap was written");

    // a device with no key: locked
    assert!(matches!(
        wraps_permit(Op::NewCode, &MemoryKeyring::new(), DRIVE),
        Err(DriveError::Locked { .. })
    ));
}
