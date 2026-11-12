//! Key permits: ONE observed state of a drive's key on this device, ONE pure judge of what may be
//! done to it in that state, and ONE registry of what a drive key touches (ARCH29 C7).
//!
//! Why. Seven of the eight MAJOR findings of the key review were one defect: a flow that changes or
//! deletes key material decided on its own question. `rotate` made a second new key when the journal
//! was missing (X-DELTA-AZDRIVE-KEYS-1); Remove drive trusted a stale `code_checked` after an
//! interrupted rotation (-2); setup deleted the only copy of the key it had generated (-3); a planted
//! journal stopped "I was hacked" before its lockdown (-6); the recovery wrap was replaced before its
//! new code could be shown (-7); Remove erased months only this computer held (-8). Each fix wrote a
//! local observer of its own (`rotation::start`, `rotation::unfinished`, `device::setup_pending`,
//! `encryption::proof_for_removal`), each right for its flow, none knowing the others - and three
//! lists of "what a drive key touches" (the keyring entries, what a rotation reseals, what a removal
//! erases) that disagreed in exactly the consumers one of them forgot (-5, -27, -28, -30, -32).
//!
//! What is here:
//!
//! - [`ask`], THE question: `(Op, Evidence)` to a [`Verdict`]. The state of the drive's key on this
//!   device is OBSERVED inside it when the operation runs and never stored (a stored flag was KEYS-2): the
//!   facts a caller gathered ([`Evidence`]) are turned into one of seven states, an unread fact is
//!   [`Read::Failed`] and lands in `Unknown`, never in "no key" or "no rotation", and the state is judged
//!   for the operation - a total function of all the cells, with the sentence the window shows in every
//!   refusal. The state and the judge are PRIVATE to this module: a caller can ask, it cannot make a state
//!   up. The truth table is the specification (`tests/keystate_table.rs`, a child of this module, lists
//!   every cell by an exhaustive match without a catch-all).
//! - [`Permit`], the verdict that lets an operation through. Only this module makes one (its
//!   constructor is private), so a function that takes a `&Permit` cannot be handed one by a flow that
//!   did not ask: `rotate`, `reencrypt_pass`, the removal's [`crate::footprint::RemovalProof`], the
//!   recovery wraps' removal and restore take one of their own operation.
//! - The facts are witnesses: the keyring's and the bucket's are made by the gatherers of `device.rs` and
//!   `rotation.rs` (their fields are the crate's), the recovery record's and the vault's by the app that
//!   keeps them ([`RecoveryRecord`], [`PaidMonths`], [`TokenFacts::of`], [`VaultFacts::of`]).
//! - [`CONSUMERS`], what a drive key touches, each with its reseal and its forget, and
//!   `tests/key_consumers_gate.rs` of azul-srcgate, which fails when a function that seals or opens
//!   with the drive key is not listed.
//!
//! Decisions of this module, each a cell or a fact the key review did not settle (the table's own
//! cells are in the tests):
//!
//! - A device that holds no key gets one with `Recover`, whichever way: the recovery code, its member
//!   wrap, or a join code's invite (`adopt_invite`). `Enroll` is the member wrap of a key the device
//!   HOLDS (setup's last step, a rotation's re-enrolment). So `Locked x Enroll` is refused and a join
//!   is not.
//! - `Unknown` refuses every operation but the lockdown: a failed read is no "nothing there". The
//!   lockdown is permitted whatever could not be read (KEYS-6: a hostile bucket must not stop it).
//! - A fact the caller did not read is [`Read::Unasked`]. One the STATE rests on (the keyring's
//!   entries) makes the state `Unknown` ([`Why::NotAsked`]); one the CELL rests on - whether the
//!   keyring keeps keys, whether the index opens, how many further codes there are, the recovery
//!   record's check and registration, the vault's months - refuses an operation that changes or
//!   erases key material ([`Refusal::NotAsked`]): a permit made by gathering less would be a proof
//!   made from a name. A journal nobody looked at is
//!   never taken for the device's own: only a journal that is (`Journal::Own`) makes a rotation
//!   resume, and resuming skips the account's lockdown.
//! - Removing a drive is permitted only with a recovery code typed back right (not by a clock that
//!   ran ahead), the token server's knowledge of it (a registration that was never recorded is not
//!   "registered"), and the months paid kept elsewhere. A planned key may be the only copy: nothing
//!   destructive is permitted while one is kept (KEYS-3).

use std::{fmt, sync::Arc};

use super::{
    device::{self, MAX_EXTRA_RECOVERY_CODES},
    drops,
    keys::{RecoveryCode, RecoveryKdf, RecoveryWrap},
    DriveKey, KeyId,
};
use crate::{
    encrypted::IndexProvider, footprint::KeyEntry, keyring::KeyringStore, rotation, sharing, Drive,
    DriveError,
};

/// The table of the judge, the observation of the state and the cells that depend on what was read: the
/// specification. A CHILD module, so that it - and nothing else - reaches the state and the judge.
#[cfg(test)]
#[path = "../tests/keystate_table.rs"]
mod table;

// ==== What was read ====

/// What reading one fact gave.
#[derive(Clone, Debug, PartialEq, Eq)]
#[must_use]
pub enum Read<T> {
    /// The fact.
    Found(T),
    /// Looked, and there is nothing: no entry, no file, no record.
    Absent,
    /// Looked, and could not tell: a keyring that refused, a bucket that did not answer. The
    /// sentence says what failed (never a secret). Never read as [`Read::Absent`].
    Failed(String),
    /// Not looked at: the caller's decision does not rest on it and it did not read it. The default
    /// of every fact, so a caller names what it gathered and nothing else.
    Unasked,
}

// A derive would ask `T: Default` of every fact; "nothing was asked" needs no value.
#[allow(clippy::derivable_impls)]
impl<T> Default for Read<T> {
    fn default() -> Read<T> {
        Read::Unasked
    }
}

impl<T> Read<T> {
    /// The read of a thing that may be missing: `Ok(Some)` is found, `Ok(None)` is absent and an
    /// `Err` is a read that failed.
    pub fn of<E: fmt::Display>(read: Result<Option<T>, E>) -> Read<T> {
        match read {
            Ok(Some(found)) => Read::Found(found),
            Ok(None) => Read::Absent,
            Err(e) => Read::Failed(e.to_string()),
        }
    }

    /// The read of a thing that is there whenever the read works.
    pub fn value<E: fmt::Display>(read: Result<T, E>) -> Read<T> {
        match read {
            Ok(found) => Read::Found(found),
            Err(e) => Read::Failed(e.to_string()),
        }
    }

    /// Whether the caller looked.
    #[must_use]
    pub fn is_asked(&self) -> bool {
        !matches!(self, Read::Unasked)
    }
}

/// The read of a drive key as the read of its id: the evidence holds ids and never keys.
pub fn key_id<E: fmt::Display>(read: &Result<Option<DriveKey>, E>) -> Read<KeyId> {
    match read {
        Ok(Some(key)) => Read::Found(key.id()),
        Ok(None) => Read::Absent,
        Err(e) => Read::Failed(e.to_string()),
    }
}

// ==== The state ====

/// Why a state is torn or unknown: each a sentence for the window ([`Refusal::sentence`]).
#[derive(Clone, Debug, PartialEq, Eq)]
enum Why {
    /// The drive's index opens with none of the drive keys this device holds: its keys were changed
    /// from another device (`rotation::opens_nothing`).
    OpensNothing,
    /// The keyring holds the new key of a rotation and not the drive key it replaces: its copy of the
    /// new key may be the only one.
    OldKeyGone,
    /// This device's own journal names a rotation, and the key it replaces is not in the keyring
    /// (`drive-key-previous`): the rotation cannot go on from here, and nothing else can finish it for
    /// this device - the recovery code is the way back in.
    CannotResume,
    /// A fact the state rests on could not be read: what, and why.
    Unreadable { what: &'static str, why: String },
    /// The caller did not read a fact the state rests on.
    NotAsked(&'static str),
}

/// How a rotation that is under way goes on. Decided from the keyring and the bucket alone: nothing
/// is written for it (`rotation::start`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Start {
    /// The journal in the bucket is this device's own - its MAC verifies under the new key the
    /// keyring keeps: the rotation goes on where it stopped, and has had its lockdown.
    Resume,
    /// This device keeps the new key of an earlier try and the bucket's journal is not its own
    /// (deleted, planted, another device's, junk - or not looked at): the new key is the rotation's
    /// again, a journal is written anew, every step runs again, and the lockdown is not skipped. No
    /// second new key is made: the first may seal the drive's index already.
    Adopt,
    /// The new key IS the drive key and the key it replaced is still in the keyring: a rotation of
    /// this device went past its Members step (the drive key became the new one, the recovery wrap
    /// still seals the old) and its journal is not the device's own any more - or a finished one
    /// whose cleanup took neither entry away. Nothing here is erased while it stands (the keyring's
    /// copy of the new key may be the only one), and a key change from here is a new one - a new key
    /// made, the lockdown run again - exactly as `rotation::start` answers `Fresh` for it.
    Again,
}

/// A drive's key on this device, as observed when an operation runs. Never stored.
#[derive(Clone, Debug, PartialEq, Eq)]
enum KeyState {
    /// No key here, and the drive is not known to be encrypted: a new drive, or one made before the
    /// encryption.
    Fresh,
    /// A key kept for the drive's setup and no drive key yet: a setup of this device that did not
    /// finish (or a checkout planned the key). The planned key may be the ONLY copy.
    Planned,
    /// This device holds the drive key and no key change is under way.
    Active,
    /// This device keeps the new key of a rotation that is not the drive key yet.
    Rotating(Start),
    /// The keys on this device cannot finish or start anything: see [`Why`].
    Torn(Why),
    /// The drive is encrypted and this device holds no key for it.
    Locked,
    /// A fact the state rests on could not be read (or was not asked). NEVER [`KeyState::Active`].
    Unknown(Why),
}

impl KeyState {
    /// The state's row of the table: its variant, whatever its payload says.
    #[cfg(test)]
    fn row(&self) -> &'static str {
        match self {
            KeyState::Fresh => "fresh",
            KeyState::Planned => "planned",
            KeyState::Active => "active",
            KeyState::Rotating(_) => "rotating",
            KeyState::Torn(_) => "torn",
            KeyState::Locked => "locked",
            KeyState::Unknown(_) => "unknown",
        }
    }
}

// ==== The evidence ====

/// Whether a keyring takes key material (`Protection::keeps_key_material`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Keeps {
    /// It does.
    Yes,
    /// It does not: the refusal's sentence (`Protection::refusal`).
    No(&'static str),
}

/// What the keyring says (`azul-storage/<kind>/<drive>`; `device::key_facts` reads them).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct KeyringFacts {
    /// The id of the key under `drive-key`.
    pub(crate) drive_key: Read<KeyId>,
    /// The id of the key under `drive-key-next`: the new key of a rotation.
    pub(crate) next_key: Read<KeyId>,
    /// The id of the key under `drive-key-previous`: the drive key a rotation replaces, kept until its
    /// last step. With the new key it tells a rotation that went past its Members step (the drive key
    /// IS the new key and this entry is still there) from a finished one that left its new key's
    /// entry behind.
    pub(crate) previous_key: Read<KeyId>,
    /// The id of the key under `drive-key-planned`: a key made before the drive's setup finished.
    pub(crate) planned_key: Read<KeyId>,
    /// Whether the keyring takes key material at all (a plain file the user did not allow does
    /// not).
    pub(crate) keeps_keys: Read<Keeps>,
}

/// Whose a rotation journal in the bucket is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Journal {
    /// Its MAC verifies under the new key this device keeps: this device wrote it.
    Own,
    /// A journal that is no rotation of this device: planted, another device's, damaged, too big.
    NotOwn,
}

/// What the bucket says.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct BucketFacts {
    /// Whether the drive is encrypted as this device knows it: the bucket lists key files, or the
    /// device recorded that it found the drive encrypted. `Absent` is `Found(false)`.
    pub(crate) encrypted: Read<bool>,
    /// The rotation's journal. `Absent`: there is none.
    pub(crate) journal: Read<Journal>,
    /// Whether the drive's index opens with one of the keys this device holds (`Found(true)` for a
    /// drive with no index yet: nothing in it is sealed). `Found(false)`: it opens with none.
    pub(crate) index_opens: Read<bool>,
    /// How many further recovery codes (second kits) the drive holds.
    pub(crate) extra_codes: Read<u32>,
}

/// The recovery record of a drive: what the app that keeps it knows of the drive's recovery code.
/// [`TokenFacts::of`] asks it and nothing else makes the facts, so the judge is never handed a stamp or a
/// flag a caller made up (the removal's proof used to be a name: a drive id and the belief of one line).
pub trait RecoveryRecord {
    /// When the recovery code was last typed back right (seconds since 1970, this computer's clock);
    /// `Absent`: never; `Failed`: the record could not be read.
    fn code_checked(&self) -> Read<u64>;
    /// Whether the token server knows the code. `Found(false)`: its registration is pending or missing;
    /// `Absent`: it was never recorded either way.
    fn code_registered(&self) -> Read<bool>;
}

/// What the token server and the recovery record say of the drive's recovery code: the answers of a
/// [`RecoveryRecord`].
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TokenFacts {
    code_checked: Read<u64>,
    code_registered: Read<bool>,
}

impl TokenFacts {
    /// What `record` says now.
    #[must_use]
    pub fn of(record: &dyn RecoveryRecord) -> TokenFacts {
        TokenFacts {
            code_checked: record.code_checked(),
            code_registered: record.code_registered(),
        }
    }

    /// A drive with no recovery record: its code was never typed back right and the token server was
    /// never told of it (asked, and the answer is "never": a removal of it keeps its keys).
    #[must_use]
    pub fn no_record() -> TokenFacts {
        TokenFacts {
            code_checked: Read::Absent,
            code_registered: Read::Absent,
        }
    }

    /// Why the keys of a removal at `now` would stay for the code's sake alone: it was never typed back
    /// right (or the check is dated after `now`), or the token server is not known to have it. `None`: the
    /// record lets them go - the keyring and the vault still decide. What the confirm dialog says before
    /// the button is pressed, and what a removal checks before it asks the drive's vault.
    #[must_use]
    pub fn refusal(&self, now: u64) -> Option<Refusal> {
        code_refusal(self, now)
    }
}

/// This computer's vault of paid months and the drive's own: what a [`PaidMonths`] says.
pub trait PaidMonths {
    /// Whether the drive's own vault holds the months this computer's vault holds, for `drive` (what
    /// it lacks is written into it once more, and read back); `Failed`: it could not be told.
    fn held_elsewhere(&self, drive: &str) -> Read<bool>;
}

/// What this computer's vault of paid months and the drive's own say.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct VaultFacts {
    months_elsewhere: Read<bool>,
}

impl VaultFacts {
    /// What `vault` says of the drive `drive`.
    #[must_use]
    pub fn of(vault: &dyn PaidMonths, drive: &str) -> VaultFacts {
        VaultFacts {
            months_elsewhere: vault.held_elsewhere(drive),
        }
    }

    /// A drive with no vault of its own to ask: nothing is paid for it, so nothing is lost.
    #[must_use]
    pub fn no_vault() -> VaultFacts {
        VaultFacts {
            months_elsewhere: Read::Absent,
        }
    }
}

/// Everything the judge is told. The callers gather it; the judge reads nothing.
#[derive(Clone, Debug)]
pub struct Evidence<'a> {
    /// The drive's id: what a [`Permit`] is for.
    pub drive: &'a str,
    pub keyring: KeyringFacts,
    pub bucket: BucketFacts,
    pub token: TokenFacts,
    pub vault: VaultFacts,
    /// Seconds since 1970 by this computer's clock: what a stamp of the recovery record is judged
    /// against (a stamp after it is no check).
    pub now: u64,
}

impl<'a> Evidence<'a> {
    /// Evidence of the drive `drive` at `now` in which nothing was asked.
    #[must_use]
    pub fn new(drive: &'a str, now: u64) -> Evidence<'a> {
        Evidence {
            drive,
            keyring: KeyringFacts::default(),
            bucket: BucketFacts::default(),
            token: TokenFacts::default(),
            vault: VaultFacts::default(),
            now,
        }
    }
}

// ==== observe ====

fn unreadable(what: &'static str, why: &str) -> KeyState {
    KeyState::Unknown(Why::Unreadable {
        what,
        why: why.to_string(),
    })
}

/// One key entry as the state needs it: the id, `None` for no entry, or the state it makes the
/// whole observation (a failed read, an entry nobody asked).
fn entry(read: &Read<KeyId>, what: &'static str) -> Result<Option<KeyId>, KeyState> {
    match read {
        Read::Found(id) => Ok(Some(*id)),
        Read::Absent => Ok(None),
        Read::Failed(why) => Err(unreadable(what, why)),
        Read::Unasked => Err(KeyState::Unknown(Why::NotAsked(what))),
    }
}

/// The state of a drive whose keys this device holds, or of a rotation it adopts: torn when the
/// drive's index opens with none of them.
fn if_the_index_opens(e: &Evidence<'_>, then: KeyState) -> KeyState {
    match &e.bucket.index_opens {
        Read::Found(false) => KeyState::Torn(Why::OpensNothing),
        Read::Failed(why) => unreadable(
            "whether the drive's index opens with this device's keys",
            why,
        ),
        Read::Found(true) | Read::Absent | Read::Unasked => then,
    }
}

/// What whether the drive is encrypted is asked as.
const ENCRYPTED: &str = "whether the drive is encrypted";
/// What the key under `drive-key` is asked as.
const DRIVE_KEY: &str = "the drive key";
/// What the key under `drive-key-next` is asked as.
const NEW_KEY: &str = "the new key of a key change";
/// What the key under `drive-key-previous` is asked as.
const PREVIOUS_KEY: &str = "the key a key change replaces";
/// What the key under `drive-key-planned` is asked as.
const PLANNED_KEY: &str = "the key kept for the drive's setup";
/// What the key change's journal is asked as.
const JOURNAL: &str = "the key change's journal";

/// The state of a device that holds no key and keeps none for a rotation.
fn without_a_key(e: &Evidence<'_>) -> KeyState {
    match &e.keyring.planned_key {
        Read::Found(_) => return KeyState::Planned,
        Read::Failed(why) => return unreadable(PLANNED_KEY, why),
        // Not looked at: "no planned key" would make a drive whose setup is unfinished a new one.
        Read::Unasked => return KeyState::Unknown(Why::NotAsked(PLANNED_KEY)),
        Read::Absent => {}
    }
    match &e.bucket.encrypted {
        Read::Found(true) => KeyState::Locked,
        Read::Found(false) | Read::Absent => KeyState::Fresh,
        Read::Failed(why) => unreadable(ENCRYPTED, why),
        Read::Unasked => KeyState::Unknown(Why::NotAsked(ENCRYPTED)),
    }
}

/// The state the facts make. TOTAL: every failed read lands in [`KeyState::Unknown`], every fact
/// the state rests on that nobody asked in [`Why::NotAsked`]; nothing is read.
///
/// It is the union of the local observers, in their own order:
///
/// - `rotation::resumable`: the new key in the keyring and a journal of this device's own make a
///   rotation to resume - asked of the drive key never (a rotation whose old key is gone fails later,
///   at `rotate`'s read of `drive-key-previous`);
/// - `rotation::start`: otherwise no drive key is `missing("old")`, a new key that is not the drive
///   key is adopted and a new key that IS the drive key (a finished rotation left its entries) is a
///   rotation that starts; both need the index to open with a key this device holds, else
///   `opens_nothing`;
/// - `rotation::unfinished`: a new key kept that is not the drive key;
/// - `device::setup_pending`: a planned key;
/// - `encrypted::Facts::gather`: no key, no planned key: the bucket's key files and the device's
///   record say locked or fresh.
#[must_use]
fn observe(e: &Evidence<'_>) -> KeyState {
    let k = &e.keyring;
    let drive = match entry(&k.drive_key, DRIVE_KEY) {
        Ok(id) => id,
        Err(state) => return state,
    };
    let next = match entry(&k.next_key, NEW_KEY) {
        Ok(id) => id,
        Err(state) => return state,
    };
    let Some(next) = next else {
        return match drive {
            Some(_) => if_the_index_opens(e, KeyState::Active),
            None => without_a_key(e),
        };
    };
    match &e.bucket.journal {
        Read::Failed(why) => return unreadable(JOURNAL, why),
        // A journal of this device's own is a rotation it resumes - from `drive-key-previous`, which
        // `rotate` reads for the key it replaces: without it the journal is this device's and the rotation
        // cannot go on from here (X-DELTA-STRUCT-KEYSTATE-4). With the new key already the drive key it is
        // a finished rotation whose cleanup took the previous key first and left the journal.
        Read::Found(Journal::Own) => {
            return match entry(&k.previous_key, PREVIOUS_KEY) {
                Err(state) => state,
                Ok(Some(_)) => KeyState::Rotating(Start::Resume),
                Ok(None) if drive == Some(next) => if_the_index_opens(e, KeyState::Active),
                Ok(None) => KeyState::Torn(Why::CannotResume),
            };
        }
        Read::Found(Journal::NotOwn) | Read::Absent | Read::Unasked => {}
    }
    match drive {
        None => KeyState::Torn(Why::OldKeyGone),
        Some(drive) if drive != next => if_the_index_opens(e, KeyState::Rotating(Start::Adopt)),
        // The new key IS the drive key (the Members step made it so). The key it replaced is deleted
        // just before the new key's own entry, so: still there - the key change went past that step
        // and is not over (the recovery wrap seals the old key until its step), and the journal says
        // whose it is; gone - a finished key change that left its entry behind (X-DELTA-STRUCT-KEYSTATE-1).
        Some(_) => match entry(&k.previous_key, PREVIOUS_KEY) {
            Err(state) => state,
            Ok(None) => if_the_index_opens(e, KeyState::Active),
            Ok(Some(_)) => match &e.bucket.journal {
                // Not looked at: this is a rotation of this device or a finished one, and the journal
                // is what tells (nothing is erased on a guess).
                Read::Unasked => KeyState::Unknown(Why::NotAsked(JOURNAL)),
                _ => if_the_index_opens(e, KeyState::Rotating(Start::Again)),
            },
        },
    }
}

// ==== The operations and the verdicts ====

/// The things that touch a drive's key.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Op {
    /// The first device turns encryption on: a new key (or the planned one), the recovery wrap.
    Setup,
    /// This device becomes a member with a key it holds (the member wrap and the keyring's drive
    /// key). Setup's last step and a rotation's re-enrolment; the way a device WITHOUT a key gets one
    /// is [`Op::Recover`].
    Enroll,
    /// A new drive key after a compromise (`rotation::rotate`).
    Rotate,
    /// The drive leaves this computer WITH its keys: the keys, the device key and the vault of paid
    /// months go with it (`footprint::forget_drive` with a `RemovalProof`).
    Remove,
    /// A further recovery code (a second kit) is made.
    AddCode,
    /// A further recovery code's wrap is taken out of the bucket.
    RemoveCode,
    /// The first recovery code's wrap is replaced by a new code's: the code on a new recovery sheet.
    /// Like [`Op::AddCode`] it writes a wrap and no key, and a key change that is not over may replace
    /// the wrap again (KEYS-7); it has no count of codes to keep.
    NewCode,
    /// The drive key is handed to another device (a join code's one-time wrap): in a key change that is
    /// not over it would hand out the key the change is replacing, and the change deletes the invite.
    Invite,
    /// A member (an invite included) is taken out of the drive: its wrap leaves the bucket.
    RemoveMember,
    /// A device without the key gets it: the recovery code, this device's member wrap in the bucket,
    /// a join code's invite.
    Recover,
    /// "Re-encrypt everything": every file into a new object with a new file key.
    Reencrypt,
    /// Files put back as of a time (the restore flows).
    RestoreAsOf,
    /// The account's lockdown: every other device's token, key and link revoked at the token server.
    Lockdown,
}

impl Op {
    /// Every operation, in the table's column order.
    pub const ALL: [Op; 13] = [
        Op::Setup,
        Op::Enroll,
        Op::Rotate,
        Op::Remove,
        Op::AddCode,
        Op::RemoveCode,
        Op::NewCode,
        Op::Invite,
        Op::RemoveMember,
        Op::Recover,
        Op::Reencrypt,
        Op::RestoreAsOf,
        Op::Lockdown,
    ];

    /// The operation's word in a sentence or a log line.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Op::Setup => "setup",
            Op::Enroll => "enroll",
            Op::Rotate => "rotate",
            Op::Remove => "remove",
            Op::AddCode => "add-code",
            Op::RemoveCode => "remove-code",
            Op::NewCode => "new-code",
            Op::Invite => "invite",
            Op::RemoveMember => "remove-member",
            Op::Recover => "recover",
            Op::Reencrypt => "reencrypt",
            Op::RestoreAsOf => "restore-as-of",
            Op::Lockdown => "lockdown",
        }
    }
}

/// How a permitted operation goes on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Way {
    /// A new start: setup makes a key, rotate makes the new key.
    New,
    /// Setup finishes the setup this device began: its planned key is kept, no second key is made.
    Finish,
    /// Rotate goes on from the journal this device wrote ([`Start::Resume`]).
    Resume,
    /// Rotate goes on with the new key of an earlier try whose journal is gone ([`Start::Adopt`]).
    Adopt,
    /// The operation has one way.
    Plain,
}

/// What stays as it is after a permitted operation: what the window says before it is pressed.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stays {
    /// The drive's data and key files in its bucket: the other devices and the recovery code open
    /// them still.
    TheBucket,
    /// The files of a synced drive, where they are on this computer.
    SyncedFiles,
    /// The keys this computer holds for someone else's recovery code (a trusted contact's): they
    /// belong to the owner whose share they hold, not to the drive.
    ContactKeys,
    /// The id of the drive key the drives file recorded: a restore puts files back and leaves the
    /// key alone, so the next open still checks the key against it (KEYS-18).
    RecordedKeyId,
}

/// What stays after a drive's removal ([`Op::Remove`]).
const REMOVAL_STAYS: &[Stays] = &[Stays::TheBucket, Stays::SyncedFiles, Stays::ContactKeys];
/// What stays after a restore ([`Op::RestoreAsOf`]).
const RESTORE_STAYS: &[Stays] = &[Stays::RecordedKeyId];

/// Why an operation is refused. [`Refusal::sentence`] says it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refusal {
    /// A fact the decision rests on could not be read.
    Unreadable { what: &'static str, why: String },
    /// The caller did not read a fact the decision rests on.
    NotAsked(&'static str),
    /// The drive is encrypted and this device holds no key for it.
    Locked,
    /// This device holds no key to do it with.
    NoKey,
    /// The drive is encrypted already (setup).
    EncryptedAlready,
    /// A setup of this device did not finish: its key may be the only copy.
    SetupUnfinished,
    /// A key change of this device did not finish: the keyring's copy of the new key may be the
    /// only one.
    KeyChangeUnfinished,
    /// The recovery wrap may change before the key change is done: a code added or removed now
    /// would open or close a way the new key's code does not (KEYS-7).
    WrapMayChange,
    /// The drive's index opens with none of the keys this device holds.
    OpensNothing,
    /// The new key of a rotation is kept and the drive key it replaces is not.
    OldKeyGone,
    /// A rotation of this device cannot go on: the key it replaces is not in the keyring.
    CannotResume,
    /// The keyring takes no key material: the sentence of its protection.
    KeyringKeepsNoKeys(&'static str),
    /// The recovery code was never typed back right (or its stamp is from the future).
    CodeNotChecked,
    /// The token server is not known to have the recovery code.
    CodeNotRegistered,
    /// The months paid for the drive are only in this computer's vault.
    MonthsOnlyHere,
    /// The drive holds as many further recovery codes as it may.
    TooManyCodes,
    /// The drive is not encrypted (yet): it has no history to restore from.
    NotEncrypted,
    /// The lockdown of this key change was done: a rotation of this device that stopped has had it.
    AlreadyLockedDown,
}

impl Refusal {
    /// The refusal of a state that is torn or unknown.
    fn of(why: &Why) -> Refusal {
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

    /// The sentence the window shows, for the drive `drive`. Where a sentence exists already in the
    /// flow that refuses today, it is that one (called, not copied: `DriveError::Locked`,
    /// `rotation::OPENS_NOTHING`); the others are said where the pair test of `tests/keystate.rs`
    /// puts them beside the flow's.
    #[must_use]
    pub fn sentence(&self, drive: &str) -> String {
        match self {
            Refusal::Unreadable { what, why } => {
                format!("{what} could not be read ({why}): nothing is changed until it can be")
            }
            Refusal::NotAsked(what) => {
                format!("{what} was not looked at: the operation is not judged without it")
            }
            Refusal::Locked => DriveError::Locked {
                drive: drive.to_string(),
            }
            .to_string(),
            Refusal::NoKey => format!("this device holds no key for \"{drive}\": there is nothing to do it with"),
            Refusal::EncryptedAlready => format!(
                "\"{drive}\" is encrypted already: open it with this device's key, an invite or the \
                 recovery code"
            ),
            Refusal::SetupUnfinished => format!(
                "this device began the setup of \"{drive}\" and did not finish it: the key it kept \
                 may be the only copy, so nothing else is done with the drive until the setup is \
                 finished (run it again)"
            ),
            Refusal::KeyChangeUnfinished => format!(
                "a key change of \"{drive}\" is not finished on this device: the keyring holds the new \
                 key of a rotation that is not the drive key yet, and its copy may be the only one, so \
                 nothing is erased or changed until the rotation is finished (run it again to resume)"
            ),
            Refusal::WrapMayChange => format!(
                "the recovery wrap of \"{drive}\" may change before its key change is finished: a \
                 recovery code added or removed now would open or close a way that the new key's \
                 code does not, so it waits until the rotation is finished"
            ),
            Refusal::OpensNothing => rotation::OPENS_NOTHING.to_string(),
            Refusal::CannotResume => format!(
                "the key change of \"{drive}\" that this device began cannot go on from here: the key it \
                 replaces is not in the keyring any more, so finish it on the device that holds that key, \
                 or unlock the drive with its recovery code"
            ),
            Refusal::OldKeyGone => format!(
                "this device keeps the new key of a key change of \"{drive}\" and not the drive key it \
                 replaces: finish the change on the device that started it, or unlock the drive with \
                 its recovery code"
            ),
            Refusal::KeyringKeepsNoKeys(why) => {
                format!("the drive key of \"{drive}\" is not kept: {why}")
            }
            Refusal::CodeNotChecked => format!(
                "the recovery code of \"{drive}\" was never typed back right (or the check is dated \
                 after now): the keys here may be the only way into the drive, so they stay"
            ),
            Refusal::CodeNotRegistered => format!(
                "the token server is not known to have the recovery code of \"{drive}\" (its \
                 registration is pending, missing or was never recorded): a code it cannot find is \
                 no way back in, so the keys stay"
            ),
            Refusal::MonthsOnlyHere => format!(
                "the months paid for \"{drive}\" are only in this computer's vault: they stay with \
                 the keys"
            ),
            Refusal::TooManyCodes => format!(
                "a drive holds at most {MAX_EXTRA_RECOVERY_CODES} further recovery codes: remove one \
                 first"
            ),
            Refusal::NotEncrypted => format!(
                "\"{drive}\" is not encrypted (yet): it has no history to restore from"
            ),
            Refusal::AlreadyLockedDown => format!(
                "the lockdown of this key change of \"{drive}\" was done already: a rotation of this \
                 device that stopped has had its lockdown, and it is not repeated"
            ),
        }
    }
}

/// An operation refused, with the sentence the window shows.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Reason {
    pub kind: Refusal,
    pub sentence: String,
}

impl Reason {
    fn of(kind: Refusal, drive: &str) -> Reason {
        let sentence = kind.sentence(drive);
        Reason { kind, sentence }
    }

    /// The refusal as the error a flow returns, in the class its callers knew: a drive this device
    /// holds no key of is [`DriveError::Locked`], an encrypted drive and a keyring that keeps no key are
    /// `InvalidConfig` (what setup said), too many further codes `Unsupported`, a fact that could not be
    /// read `Io`, and every other refusal `Denied` with its sentence. A flow that holds the error of the
    /// read that failed (the keyring's, the bucket's) returns that one instead.
    #[must_use]
    pub fn error(&self, drive: &str) -> DriveError {
        match &self.kind {
            Refusal::Locked => DriveError::Locked {
                drive: drive.to_string(),
            },
            Refusal::EncryptedAlready | Refusal::KeyringKeepsNoKeys(_) => {
                DriveError::InvalidConfig(self.sentence.clone())
            }
            Refusal::TooManyCodes => DriveError::Unsupported(self.sentence.clone()),
            Refusal::Unreadable { .. } => DriveError::Io(self.sentence.clone()),
            _ => DriveError::Denied {
                message: self.sentence.clone(),
            },
        }
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.sentence)
    }
}

/// The evidence that an operation may go on: what [`judge`] hands out. Its constructor is private to
/// this module and it has no `Clone`, `Default` or `Deserialize`: whoever holds one asked the judge
/// about this drive and this operation. A function that changes or erases key material takes a
/// `&Permit` (C7 step 3; none does yet).
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub struct Permit {
    op: Op,
    drive: String,
    /// The id of the drive key the keyring held when it was asked: a permit is for THAT key. A function
    /// that works on a drive opened with a key checks it ([`Permit::allows_key`]).
    key: Option<KeyId>,
    way: Way,
    stays: &'static [Stays],
}

impl Permit {
    /// `Ok` when this permit is for `op` of `drive`: what a function that takes a permit checks first.
    /// A permit of another operation, or for another drive, permits nothing here.
    ///
    /// # Errors
    ///
    /// `Denied`, naming what the permit is for.
    pub fn allows(&self, op: Op, drive: &str) -> Result<(), DriveError> {
        if self.op == op && self.drive == drive {
            Ok(())
        } else {
            Err(DriveError::Denied {
                message: format!(
                    "the permit is for {} of \"{}\", not for {} of \"{drive}\"",
                    self.op.name(),
                    self.drive,
                    op.name()
                ),
            })
        }
    }

    /// `Ok` when this permit is for `op` and was asked while the keyring held the drive key `key`: what a
    /// function that works on a drive OPENED with a key checks (the drive's own name is not known to it).
    ///
    /// # Errors
    ///
    /// `Denied`, naming what the permit is for.
    pub fn allows_key(&self, op: Op, key: KeyId) -> Result<(), DriveError> {
        if self.op == op && self.key == Some(key) {
            Ok(())
        } else {
            Err(DriveError::Denied {
                message: format!(
                    "the permit is for {} of \"{}\" under another drive key, not for {} of the drive \
                     key {key}",
                    self.op.name(),
                    self.drive,
                    op.name()
                ),
            })
        }
    }

    /// `Ok` when this permit is for `op`, whatever the drive: what a function checks that is not told
    /// which drive it works on (a kit's wrap is taken out of the bucket it is handed).
    ///
    /// # Errors
    ///
    /// `Denied`, naming what the permit is for.
    pub fn allows_op(&self, op: Op) -> Result<(), DriveError> {
        if self.op == op {
            Ok(())
        } else {
            Err(DriveError::Denied {
                message: format!(
                    "the permit is for {} of \"{}\", not for {}",
                    self.op.name(),
                    self.drive,
                    op.name()
                ),
            })
        }
    }

    /// The operation it permits.
    #[must_use]
    pub fn op(&self) -> Op {
        self.op
    }

    /// The drive it is for.
    #[must_use]
    pub fn drive(&self) -> &str {
        &self.drive
    }

    /// How the operation goes on.
    #[must_use]
    pub fn way(&self) -> Way {
        self.way
    }

    /// What stays as it is afterwards.
    #[must_use]
    pub fn stays(&self) -> &'static [Stays] {
        self.stays
    }
}

/// The judge's answer.
#[derive(Debug, PartialEq, Eq)]
#[must_use]
pub enum Verdict {
    Permit(Permit),
    Refuse(Reason),
}

fn permit(op: Op, e: &Evidence<'_>, way: Way, stays: &'static [Stays]) -> Verdict {
    Verdict::Permit(Permit {
        op,
        drive: e.drive.to_string(),
        key: match &e.keyring.drive_key {
            Read::Found(id) => Some(*id),
            Read::Absent | Read::Failed(_) | Read::Unasked => None,
        },
        way,
        stays,
    })
}

fn refuse(kind: Refusal, e: &Evidence<'_>) -> Verdict {
    Verdict::Refuse(Reason::of(kind, e.drive))
}

/// The refusal for a failed read of `what`.
fn failed(what: &'static str, why: &str) -> Refusal {
    Refusal::Unreadable {
        what,
        why: why.to_string(),
    }
}

/// An operation that writes a key into the keyring is refused when the keyring keeps no key material
/// (a plain file the user did not allow, AZ-K7): asked BEFORE anything that cannot be taken back is
/// done at a server (KEYS-25). `None`: nothing in the way. Whether the keyring keeps keys is a fact
/// such an operation rests on: not looked at is not "yes" (X-DELTA-STRUCT-KEYSTATE-2).
fn keyring_refuses(e: &Evidence<'_>) -> Option<Refusal> {
    keeps_refusal(&e.keyring.keeps_keys)
}

/// [`keyring_refuses`] on the fact itself.
fn keeps_refusal(keeps: &Read<Keeps>) -> Option<Refusal> {
    match keeps {
        Read::Found(Keeps::No(why)) => Some(Refusal::KeyringKeepsNoKeys(why)),
        Read::Failed(why) => Some(failed(KEEPS, why)),
        Read::Found(Keeps::Yes) => None,
        Read::Absent | Read::Unasked => Some(Refusal::NotAsked(KEEPS)),
    }
}

/// What a flow that writes a key says FIRST, before it reads anything of the drive: a keyring that keeps
/// no key material (a plain file the user did not allow) refuses whatever else is the matter on this
/// device, in the words that tell the user what to allow - the same refusal the cells of Setup, Rotate and
/// Recover give. `None`: the keyring takes keys.
#[must_use]
pub fn keyring_refusal(drive: &str, keeps: &Read<Keeps>) -> Option<Reason> {
    keeps_refusal(keeps).map(|kind| Reason::of(kind, drive))
}

/// What a keyring's protection is asked as.
const KEEPS: &str = "whether the keyring keeps key material";
/// What the drive's index is asked as.
const INDEX_FACT: &str = "whether the drive's index opens with this device's keys";

/// A key change and a re-encryption rest on the drive's index opening with a key this device holds: a
/// bucket nobody asked is not a bucket whose index opens (X-DELTA-STRUCT-KEYSTATE-2).
fn index_not_asked(e: &Evidence<'_>) -> Option<Refusal> {
    matches!(e.bucket.index_opens, Read::Unasked).then_some(Refusal::NotAsked(INDEX_FACT))
}

/// A permit for `op`, unless the keyring takes no key material.
fn permit_keeping(op: Op, e: &Evidence<'_>, way: Way) -> Verdict {
    match keyring_refuses(e) {
        Some(kind) => refuse(kind, e),
        None => permit(op, e, way, &[]),
    }
}

/// [`permit_keeping`] for an operation that also rests on the index opening ([`index_not_asked`]).
fn permit_keeping_over_an_index(op: Op, e: &Evidence<'_>, way: Way) -> Verdict {
    match keyring_refuses(e).or_else(|| index_not_asked(e)) {
        Some(kind) => refuse(kind, e),
        None => permit(op, e, way, &[]),
    }
}

/// A permit for `op`, which writes no key and rests on the drive's index opening.
fn permit_over_an_index(op: Op, e: &Evidence<'_>) -> Verdict {
    match index_not_asked(e) {
        Some(kind) => refuse(kind, e),
        None => permit_plain(op, e),
    }
}

/// A permit for an operation with nothing else to ask.
fn permit_plain(op: Op, e: &Evidence<'_>) -> Verdict {
    permit(op, e, Way::Plain, &[])
}

/// What the recovery record's check of the code is asked as.
const CHECKED: &str = "whether the recovery code was checked";
/// What the token server's knowledge of the code is asked as.
const REGISTERED: &str = "whether the token server knows the recovery code";
/// What the drive's own vault is asked as.
const MONTHS: &str = "whether the drive's own vault holds the paid months";

/// Remove drive: the keys go with the drive only when no other way back in depends on them. The
/// recovery code was typed back right (not after `now`: a stamp from a clock that ran ahead is no
/// check), the token server knows it, and the months paid are kept elsewhere too. Each is a fact the
/// permit rests on: what was not asked refuses ([`Refusal::NotAsked`]) as what could not be read does -
/// a permit made by gathering less would be a proof made from a name (X-DELTA-STRUCT-KEYSTATE-2).
fn removal(e: &Evidence<'_>) -> Verdict {
    if let Some(kind) = code_refusal(&e.token, e.now) {
        return refuse(kind, e);
    }
    match &e.vault.months_elsewhere {
        // A drive with no vault of its own (`Absent`) has no months to lose.
        Read::Found(true) | Read::Absent => {}
        Read::Found(false) => return refuse(Refusal::MonthsOnlyHere, e),
        Read::Failed(why) => return refuse(failed(MONTHS, why), e),
        Read::Unasked => return refuse(Refusal::NotAsked(MONTHS), e),
    }
    permit(Op::Remove, e, Way::Plain, REMOVAL_STAYS)
}

/// The first two facts a removal rests on, which the recovery record answers: the code was typed back
/// right (not after `now`) and the token server knows it.
fn code_refusal(token: &TokenFacts, now: u64) -> Option<Refusal> {
    match &token.code_checked {
        Read::Found(at) if *at <= now => {}
        Read::Found(_) | Read::Absent => return Some(Refusal::CodeNotChecked),
        Read::Failed(why) => return Some(failed(CHECKED, why)),
        Read::Unasked => return Some(Refusal::NotAsked(CHECKED)),
    }
    match &token.code_registered {
        Read::Found(true) => None,
        Read::Found(false) | Read::Absent => Some(Refusal::CodeNotRegistered),
        Read::Failed(why) => Some(failed(REGISTERED, why)),
        Read::Unasked => Some(Refusal::NotAsked(REGISTERED)),
    }
}

/// Whether a removal was refused for want of the vault's answer ALONE: the record and the keyring would
/// let the keys go, and what the drive's own vault holds of the months was not asked. The caller that
/// pays for that question (it writes into the drive's vault, and reads it back) asks it only then, and
/// asks the judge again.
#[must_use]
pub fn waits_for_the_vault(verdict: &Verdict) -> bool {
    matches!(
        verdict,
        Verdict::Refuse(Reason { kind: Refusal::NotAsked(what), .. }) if *what == MONTHS
    )
}

/// What the count of a drive's further recovery codes is asked as.
const EXTRA_CODES: &str = "the drive's further recovery codes";

/// AddCode on a drive whose key this device holds: a drive holds at most
/// [`MAX_EXTRA_RECOVERY_CODES`] further codes.
fn add_code(e: &Evidence<'_>) -> Verdict {
    match &e.bucket.extra_codes {
        Read::Found(n) if usize::try_from(*n).map_or(true, |n| n >= MAX_EXTRA_RECOVERY_CODES) => {
            refuse(Refusal::TooManyCodes, e)
        }
        Read::Failed(why) => refuse(failed(EXTRA_CODES, why), e),
        // How many there are is what the cell rests on: not looked at is not "none".
        Read::Unasked => refuse(Refusal::NotAsked(EXTRA_CODES), e),
        Read::Found(_) | Read::Absent => permit_plain(Op::AddCode, e),
    }
}

fn fresh(op: Op, e: &Evidence<'_>) -> Verdict {
    match op {
        Op::Setup => permit_keeping(op, e, Way::New),
        Op::Remove => removal(e),
        Op::Recover => permit_keeping(op, e, Way::Plain),
        Op::RestoreAsOf => refuse(Refusal::NotEncrypted, e),
        Op::Enroll
        | Op::Rotate
        | Op::AddCode
        | Op::RemoveCode
        | Op::NewCode
        | Op::Invite
        | Op::RemoveMember
        | Op::Reencrypt
        | Op::Lockdown => refuse(Refusal::NoKey, e),
    }
}

fn planned(op: Op, e: &Evidence<'_>) -> Verdict {
    match op {
        Op::Setup => permit_keeping(op, e, Way::Finish),
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
        | Op::Lockdown => refuse(Refusal::SetupUnfinished, e),
    }
}

fn active(op: Op, e: &Evidence<'_>) -> Verdict {
    match op {
        // The last step of a setup that stopped between its key and the planned key's deletion: the
        // same call finishes it (the key was never shown a code) - when the key left behind IS the
        // drive key. Without one, or with a planned key of another id (a checkout's, for a drive of
        // another key), the drive is encrypted already: finishing would put a key nothing seals in the
        // drive key's place (X-DELTA-STRUCT-KEYSTATE-5).
        Op::Setup => match (&e.keyring.planned_key, &e.keyring.drive_key) {
            (Read::Found(planned), Read::Found(drive)) if planned == drive => {
                permit_keeping(op, e, Way::Finish)
            }
            _ => refuse(Refusal::EncryptedAlready, e),
        },
        Op::Enroll => permit_keeping(op, e, Way::Plain),
        Op::Rotate => permit_keeping_over_an_index(op, e, Way::New),
        Op::Remove => removal(e),
        Op::AddCode => add_code(e),
        Op::Reencrypt => permit_over_an_index(op, e),
        // A wrap written or taken out, an invite made, a member taken out: nothing of the keyring is
        // written and no fact is read.
        Op::RemoveCode | Op::NewCode | Op::Invite | Op::RemoveMember | Op::Lockdown => {
            permit_plain(op, e)
        }
        Op::Recover => permit_keeping(op, e, Way::Plain),
        Op::RestoreAsOf => permit(op, e, Way::Plain, RESTORE_STAYS),
    }
}

fn rotating(start: Start, op: Op, e: &Evidence<'_>) -> Verdict {
    match op {
        Op::Setup => refuse(Refusal::EncryptedAlready, e),
        Op::Enroll => permit_keeping(op, e, Way::Plain),
        Op::Rotate => match start {
            // A journal of this device's own goes on whatever the index says (`rotation::start`
            // returns before it reads it).
            Start::Resume => permit_keeping(op, e, Way::Resume),
            Start::Adopt => permit_keeping_over_an_index(op, e, Way::Adopt),
            // `rotation::start` answers `Fresh` here: a new key, a new journal, the lockdown again.
            Start::Again => permit_keeping_over_an_index(op, e, Way::New),
        },
        Op::AddCode | Op::RemoveCode | Op::NewCode => refuse(Refusal::WrapMayChange, e),
        // The key change replaces the key and deletes the invites and the members it did not make:
        // an invite made now hands out the key being replaced.
        Op::Remove
        | Op::Invite
        | Op::RemoveMember
        | Op::Recover
        | Op::Reencrypt
        | Op::RestoreAsOf => refuse(Refusal::KeyChangeUnfinished, e),
        Op::Lockdown => match start {
            Start::Resume => refuse(Refusal::AlreadyLockedDown, e),
            Start::Adopt | Start::Again => permit_plain(op, e),
        },
    }
}

fn torn(why: &Why, op: Op, e: &Evidence<'_>) -> Verdict {
    match op {
        Op::Setup => refuse(Refusal::EncryptedAlready, e),
        // The way out of keys that open nothing: a code from the device that changed them, or the
        // recovery code (the sentence of `opens_nothing`).
        Op::Recover => permit_keeping(op, e, Way::Plain),
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
        | Op::Lockdown => refuse(Refusal::of(why), e),
    }
}

fn locked(op: Op, e: &Evidence<'_>) -> Verdict {
    match op {
        Op::Recover => permit_keeping(op, e, Way::Plain),
        // The setup says what it always said of an encrypted drive it has no key for: open it with
        // an invite or the recovery code.
        Op::Setup => refuse(Refusal::EncryptedAlready, e),
        // What this computer keeps of a drive it holds no drive key of (a member key, a device key, the
        // request key of a recovery, the vault's key) goes with the drive on the removal's own conditions:
        // a recovery code typed back right is a way back in whatever this keyring holds, and months that
        // are only here stay. A refusal would leave a member key behind for good after a removal the code
        // made safe. (The table said Refuse(Locked); a refused removal means "the keys stay", and these
        // are the keys.)
        Op::Remove => removal(e),
        Op::Enroll
        | Op::Rotate
        | Op::AddCode
        | Op::RemoveCode
        | Op::NewCode
        | Op::Invite
        | Op::RemoveMember
        | Op::Reencrypt
        | Op::RestoreAsOf
        | Op::Lockdown => refuse(Refusal::Locked, e),
    }
}

fn unknown(why: &Why, op: Op, e: &Evidence<'_>) -> Verdict {
    match op {
        // A failed read stops no lockdown: whoever can make the bucket or the keyring fail must not
        // be able to keep "I was hacked" from running (KEYS-6).
        Op::Lockdown => permit_plain(op, e),
        // A removal does not rest on whether the bucket lists key files: a drive with no key on this
        // computer is a fresh one or a locked one, and both go on the removal's own conditions. (The
        // removal reads no bucket: a drive that cannot be reached is removed all the same.)
        Op::Remove if *why == Why::NotAsked(ENCRYPTED) => removal(e),
        // A recovery is judged on the entry it writes - the drive key - and nothing else: whether the
        // bucket lists key files is not asked before it runs, and a key change's, a setup's or a journal's
        // entry that cannot be read does not lock out the user who has the recovery code, which is the way
        // back in (X-DELTA-STRUCT-KEYSTATE-4). A failed read of the drive key itself refuses.
        Op::Recover
            if matches!(
                why,
                Why::Unreadable { what, .. } | Why::NotAsked(what)
                    if matches!(*what, ENCRYPTED | NEW_KEY | PREVIOUS_KEY | PLANNED_KEY)
            ) =>
        {
            permit_keeping(op, e, Way::Plain)
        }
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
        | Op::RestoreAsOf => refuse(Refusal::of(why), e),
    }
}

/// What `op` may do on this device: THE question of the key permit. The state of the drive's key is
/// observed inside from the evidence ([`observe`]) and judged for `op` ([`judge`]); the caller hands
/// over what it read and never a state of its own making. The verdict is a [`Permit`] - the only way
/// to get one - or the [`Reason`] the window shows.
pub fn ask(op: Op, e: &Evidence<'_>) -> Verdict {
    judge(&observe(e), op, e)
}

/// What `op` may do in `state`, given what was read (`e`). TOTAL: all 7 x 10 cells answer, and no
/// cell is a catch-all - `tests/keystate.rs` states the table as an exhaustive match over
/// `(KeyState, Op)`.
///
/// The cells that carry a finding: Rotating x Rotate resumes or adopts, never a second new key
/// (KEYS-1); Rotating x Remove is refused (KEYS-2); Rotating x AddCode and RemoveCode are refused
/// (KEYS-7); Planned x Setup finishes and Planned x anything else is refused, a planned key may be the
/// only copy (KEYS-3); Unknown x Lockdown is permitted whatever could not be read, and Unknown x
/// everything else is refused (KEYS-6); Torn x Rotate is refused (`opens_nothing`); Active x Remove is
/// permitted only with a checked code, a registered code and the months kept elsewhere, and says what
/// stays (KEYS-8, KEYS-32); Active x RestoreAsOf is permitted and keeps the recorded key id
/// (KEYS-18); Locked x anything but Recover is refused.
fn judge(state: &KeyState, op: Op, e: &Evidence<'_>) -> Verdict {
    match state {
        KeyState::Fresh => fresh(op, e),
        KeyState::Planned => planned(op, e),
        KeyState::Active => active(op, e),
        KeyState::Rotating(start) => rotating(*start, op, e),
        KeyState::Torn(why) => torn(why, op, e),
        KeyState::Locked => locked(op, e),
        KeyState::Unknown(why) => unknown(why, op, e),
    }
}

// ==== What a drive key touches ====

/// What a key change hands each consumer to reseal: the drive, the bucket and the keyring, the
/// provider of the index, and the keys (`older` are the keys before `old`, from the key chain).
pub struct Change<'a> {
    pub drive: &'a str,
    pub bucket: &'a Arc<dyn Drive>,
    pub keyring: &'a dyn KeyringStore,
    pub provider: &'a dyn IndexProvider,
    pub old: &'a DriveKey,
    pub new: &'a DriveKey,
    pub older: &'a [DriveKey],
    pub kdf: RecoveryKdf,
}

/// What a removal hands each consumer to forget.
pub struct Erase<'a> {
    pub drive: &'a str,
    pub keyring: &'a dyn KeyringStore,
}

/// What a consumer's reseal or forget gives back. Never prints a code.
#[must_use]
pub enum Done {
    /// Done; nothing to hand on.
    Nothing,
    /// How many entries, files or links were changed.
    Count(usize),
    /// The new recovery code, for the recovery sheet: stored nowhere.
    Code(RecoveryCode),
    /// The new drop key's public half, when incoming mail is on.
    DropKey(Option<drops::DropPublic>),
    /// The other members and invites removed, and the further recovery codes' wraps.
    Removed { members: usize, codes: usize },
}

impl fmt::Debug for Done {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Done::Nothing => f.write_str("Nothing"),
            Done::Count(n) => write!(f, "Count({n})"),
            Done::Code(_) => f.write_str("Code(***)"),
            Done::DropKey(key) => write!(
                f,
                "DropKey({})",
                if key.is_some() { "some" } else { "none" }
            ),
            Done::Removed { members, codes } => {
                write!(f, "Removed {{ members: {members}, codes: {codes} }}")
            }
        }
    }
}

/// A consumer's reseal under the new key (step of a rotation).
pub type ResealFn = fn(&Permit, &Change<'_>) -> Result<Done, DriveError>;
/// A consumer's forget (step of a removal).
pub type ForgetFn = fn(&Permit, &Erase<'_>) -> Result<Done, DriveError>;

/// How a consumer is resealed by a key change.
#[derive(Clone, Copy)]
pub enum Reseal {
    /// This crate does it: the function, written from the existing code of the rotation.
    Fn(ResealFn),
    /// Another consumer's reseal does it in the same pass.
    Within(&'static str),
    /// Nothing of it is sealed under the drive key: why.
    Nothing(&'static str),
}

/// How a consumer is forgotten by a removal.
#[derive(Clone, Copy)]
pub enum Forget {
    /// This crate does it: the function, written from the existing code of the removal.
    Fn(ForgetFn),
    /// Nothing of it is on this computer, or the app's stores take it ([`Consumer::functions`] names
    /// where): why.
    Nothing(&'static str),
}

/// One thing a drive key touches.
#[derive(Clone, Copy)]
pub struct Consumer {
    pub name: &'static str,
    /// The keyring entries this consumer owns ([`owner`] says it the other way round).
    pub entries: &'static [KeyEntry],
    pub reseal: Reseal,
    pub forget: Forget,
    /// The functions that seal or open with the drive key for this consumer, each
    /// `<file from the workspace root>#<function>`. The gate `key_consumers_gate` of azul-srcgate
    /// reads this list from the source: a function that uses the drive key and is not here (or in the
    /// gate's own data path) fails it, and so does an entry that names no such function.
    pub functions: &'static [&'static str],
}

impl fmt::Debug for Consumer {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Consumer")
            .field("name", &self.name)
            .field("entries", &self.entries)
            .finish_non_exhaustive()
    }
}

/// A reseal or a forget runs only for a permit of its operation, for this drive.
fn permitted(permit: &Permit, op: Op, drive: &str) -> Result<(), DriveError> {
    permit.allows(op, drive)
}

/// Steps 2 and 3 of a rotation: the index sealed under the new key (an index that cannot be rekeyed
/// stops the rotation before anything else changed), then every entry's file key re-wrapped from the
/// old key (or an older one of the chain) to the new. `rotate` puts the chain's step between the two.
fn reseal_index(permit: &Permit, c: &Change<'_>) -> Result<Done, DriveError> {
    permitted(permit, Op::Rotate, c.drive)?;
    c.provider
        .rekey(c.drive, Arc::clone(c.bucket), c.old, c.new)?;
    let index = c
        .provider
        .open_index(c.drive, Arc::clone(c.bucket), c.new)?;
    rotation::rewrap_index_with(index.as_ref(), c.old, c.older, c.new).map(Done::Count)
}

/// Step 4: this device gets a NEW member key (the old one may have leaked with the old drive key)
/// and its wrap is sealed to the new key; every other member, every open invite and every further
/// recovery code's wrap leaves the bucket.
fn reseal_members(permit: &Permit, c: &Change<'_>) -> Result<Done, DriveError> {
    permitted(permit, Op::Rotate, c.drive)?;
    device::delete_key_at(c.keyring, &device::member_key_entry(c.drive))?;
    let me = device::enroll(c.bucket.as_ref(), c.keyring, c.drive, c.new)?;
    let (members, codes) = rotation::remove_other_members(c.bucket.as_ref(), c.new, &me)?;
    Ok(Done::Removed { members, codes })
}

/// Step 7: a new recovery code, its wrap under the new key in the old one's place - the keyring's
/// drive key must BE the new key first (the wrap is published only for a key the keyring holds). The
/// last step that can fail: from the wrap on the code is the drive's and must reach the user.
fn reseal_recovery_wrap(permit: &Permit, c: &Change<'_>) -> Result<Done, DriveError> {
    permitted(permit, Op::Rotate, c.drive)?;
    let Some((key, stashed)) = device::held(c.keyring, &device::drive_key_entry(c.drive))? else {
        return Err(DriveError::Locked {
            drive: c.drive.to_string(),
        });
    };
    if key != *c.new {
        return Err(DriveError::Io(String::from(
            "the keyring's drive key is not the new key: the recovery wrap is sealed to the key the \
             keyring holds, and only after the member step has made it the drive's",
        )));
    }
    let code = RecoveryCode::generate().map_err(|e| e.for_key(c.drive))?;
    let wrap =
        RecoveryWrap::seal(&key, c.drive, &code, c.kdf.clone()).map_err(|e| e.for_key(c.drive))?;
    device::publish_recovery_wrap(
        c.bucket.as_ref(),
        &stashed,
        &wrap,
        device::Publish::FirstReplace,
    )?;
    Ok(Done::Code(code))
}

/// Step 6: every share is revoked (its manifest deleted).
fn reseal_shares(permit: &Permit, c: &Change<'_>) -> Result<Done, DriveError> {
    permitted(permit, Op::Rotate, c.drive)?;
    sharing::revoke_all_shares(c.bucket.as_ref()).map(Done::Count)
}

/// Step 5: a new drop key; the old one is kept, sealed under the new key, for the drops the Worker
/// sealed before it had the new public half.
fn reseal_drops(permit: &Permit, c: &Change<'_>) -> Result<Done, DriveError> {
    permitted(permit, Op::Rotate, c.drive)?;
    drops::rotate_drop_key(c.bucket.as_ref(), c.old, c.new, c.drive).map(Done::DropKey)
}

/// The key chain: the old key sealed under the new one, read back before anything lets go of it.
fn reseal_chain(permit: &Permit, c: &Change<'_>) -> Result<Done, DriveError> {
    permitted(permit, Op::Rotate, c.drive)?;
    rotation::chain_old_key(c.bucket.as_ref(), c.drive, c.old, c.new).map(|()| Done::Nothing)
}

/// Takes the keyring entry `entry` of the drive away, for a permit of a removal.
fn forget_entry(permit: &Permit, e: &Erase<'_>, entry: KeyEntry) -> Result<Done, DriveError> {
    permitted(permit, Op::Remove, e.drive)?;
    device::delete_key_at(e.keyring, &entry.name(e.drive)).map(|()| Done::Nothing)
}

fn forget_member_key(permit: &Permit, e: &Erase<'_>) -> Result<Done, DriveError> {
    forget_entry(permit, e, KeyEntry::MemberKey)
}

fn forget_request_key(permit: &Permit, e: &Erase<'_>) -> Result<Done, DriveError> {
    forget_entry(permit, e, KeyEntry::RequestKey)
}

fn forget_vault_key(permit: &Permit, e: &Erase<'_>) -> Result<Done, DriveError> {
    forget_entry(permit, e, KeyEntry::VaultKey)
}

fn forget_device_key(permit: &Permit, e: &Erase<'_>) -> Result<Done, DriveError> {
    forget_entry(permit, e, KeyEntry::DeviceKey)
}

/// The drive's index: sealed under keys derived from the drive key (the sealing key and the names
/// key: `seal`, `open` and `name_hash` of the drive key's `Sealer`), and every entry holds a file
/// key the drive key wraps.
pub const INDEX: Consumer = Consumer {
    name: "INDEX",
    entries: &[],
    reseal: Reseal::Fn(reseal_index),
    forget: Forget::Nothing(
        "the index is in the bucket; the copy of it and the search index on this computer are the \
         app's stores (azul-drive footprint::Places::of)",
    ),
    functions: &[
        "examples/azul-storage/src/rotation.rs#rewrap_index_with",
        "examples/azul-storage/src/meta/seal.rs#seal",
        "examples/azul-storage/src/meta/seal.rs#open",
        "examples/azul-storage/src/meta/seal.rs#name_hash",
    ],
};

/// This device's member key and wrap, and every other member's wrap and invite.
pub const MEMBERS: Consumer = Consumer {
    name: "MEMBERS",
    entries: &[KeyEntry::MemberKey],
    reseal: Reseal::Fn(reseal_members),
    forget: Forget::Fn(forget_member_key),
    functions: &[
        "examples/azul-storage/src/crypto/device.rs#enroll",
        "examples/azul-storage/src/crypto/device.rs#seal_invite",
    ],
};

/// The first recovery code's wrap (`recovery.key`) and the request key of a recovery that waits for
/// trusted contacts.
pub const RECOVERY_WRAP: Consumer = Consumer {
    name: "RECOVERY_WRAP",
    entries: &[KeyEntry::RequestKey],
    reseal: Reseal::Fn(reseal_recovery_wrap),
    forget: Forget::Fn(forget_request_key),
    functions: &[
        "examples/azul-storage/src/crypto/device.rs#setup_new_drive_reporting",
        "examples/azul-storage/src/rotation.rs#rotate",
        "examples/azul-storage/src/crypto/keystate.rs#reseal_recovery_wrap",
        "examples/azul-drive/src/encryption.rs#run",
    ],
};

/// The further recovery codes' wraps (second kits).
pub const KITS: Consumer = Consumer {
    name: "KITS",
    entries: &[],
    reseal: Reseal::Within("MEMBERS"),
    forget: Forget::Nothing(
        "the kits' wraps are in the bucket; this computer's record of them is the recovery settings \
         (azul-drive recovery_health::RecoveryState::extra_codes)",
    ),
    functions: &["examples/azul-storage/src/crypto/device.rs#add_recovery_code_named"],
};

/// The links to the drive's files: share manifests sealed under a share key, the file keys the
/// drive key unwrapped for them.
pub const SHARES: Consumer = Consumer {
    name: "SHARES",
    entries: &[],
    reseal: Reseal::Fn(reseal_shares),
    forget: Forget::Nothing("the shares are in the bucket; nothing of them is on this computer"),
    functions: &[
        "examples/azul-storage/src/crypto/share.rs#seal",
        "examples/azul-storage/src/crypto/share.rs#open",
        "examples/azul-storage/src/sharing.rs#open_shared_entry",
        "examples/azul-storage/src/encrypted.rs#share_file",
        "examples/azul-storage/src/encrypted.rs#read_shared",
    ],
};

/// The vault of the months paid for: the drive's own, an object under the drive key (opened through
/// the key chain after a rotation, written again under the new key at its next join), and this
/// computer's, sealed under a key of its own that goes with the drive.
pub const PERIOD_VAULT: Consumer = Consumer {
    name: "PERIOD_VAULT",
    entries: &[KeyEntry::VaultKey],
    reseal: Reseal::Nothing(
        "the drive's own vault is opened through the key chain after a rotation and written again \
         under the new key at its next join (azcloud-kit vault::DriveVault)",
    ),
    forget: Forget::Fn(forget_vault_key),
    functions: &[
        "examples/azcloud-kit/src/vault.rs#open",
        "examples/azcloud-kit/src/vault.rs#seal",
    ],
};

/// The drive's drop key (incoming mail) and the ones before it, each sealed under the drive key.
pub const DROPS: Consumer = Consumer {
    name: "DROPS",
    entries: &[],
    reseal: Reseal::Fn(reseal_drops),
    forget: Forget::Nothing("the drop keys are in the bucket; nothing of them is on this computer"),
    functions: &[
        "examples/azul-storage/src/crypto/drops.rs#seal",
        "examples/azul-storage/src/crypto/drops.rs#open",
        "examples/azul-storage/src/crypto/drops.rs#rotate_drop_key",
        "examples/azul-storage/src/crypto/drops.rs#replace_drop_key",
    ],
};

/// The state of a "re-encrypt everything" pass: a pass that began before the keys were made must not
/// be taken up under the new ones (KEYS-29).
pub const REENCRYPT_STATE: Consumer = Consumer {
    name: "REENCRYPT_STATE",
    entries: &[],
    reseal: Reseal::Nothing(
        "the state is plain JSON beside the drives file; a state of an older key is not taken up \
         (azul-drive encryption::current_pass)",
    ),
    forget: Forget::Nothing(
        "the state files are the app's stores (azul-drive footprint::migration_state)",
    ),
    functions: &[],
};

/// The key chain: the old drive key sealed under the new one (`_chain.key`), and the rotation's
/// journal, authenticated with the new key (`_rotation.key`).
pub const CHAIN: Consumer = Consumer {
    name: "CHAIN",
    entries: &[],
    reseal: Reseal::Fn(reseal_chain),
    forget: Forget::Nothing("the chain and the journal are in the bucket"),
    functions: &[
        "examples/azul-storage/src/crypto/keys.rs#add",
        "examples/azul-storage/src/crypto/keys.rs#open",
        "examples/azul-storage/src/rotation.rs#chain_old_key",
        "examples/azul-storage/src/rotation.rs#mac_of",
    ],
};

/// This computer's own signing key for an Azlin drive: not sealed under the drive key, but the one
/// thing that gets a locked drive back to its owner without the recovery code, so it goes with the
/// proof like the keys (KEYS-32).
pub const DEVICE_KEY: Consumer = Consumer {
    name: "DEVICE_KEY",
    entries: &[KeyEntry::DeviceKey],
    reseal: Reseal::Nothing("an Ed25519 key of this computer, not sealed under the drive key"),
    forget: Forget::Fn(forget_device_key),
    functions: &[],
};

/// Everything a drive key touches, in the order of C7.
pub const CONSUMERS: &[Consumer] = &[
    INDEX,
    MEMBERS,
    RECOVERY_WRAP,
    KITS,
    SHARES,
    PERIOD_VAULT,
    DROPS,
    REENCRYPT_STATE,
    CHAIN,
    DEVICE_KEY,
];

/// Whose a keyring entry is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The key itself: the drive key, the two keys of a rotation, the key planned before the setup.
    /// `device::forget_keys` and `footprint::forget_drive` take them.
    TheKey,
    /// A credential for the drive's server side, not a key: the session and the drive token.
    Credential,
    /// The entry belongs to the consumer of this name.
    Consumer(&'static str),
}

/// Whose `entry` is. Exhaustive without a catch-all: a new kind of keyring entry does not compile
/// until it has an owner here - one list of "what a drive leaves in the keyring", not three.
#[must_use]
pub const fn owner(entry: KeyEntry) -> Owner {
    match entry {
        KeyEntry::DriveKey
        | KeyEntry::RotationPrevious
        | KeyEntry::RotationNext
        | KeyEntry::PlannedKey => Owner::TheKey,
        KeyEntry::Session | KeyEntry::AzlinToken => Owner::Credential,
        KeyEntry::MemberKey => Owner::Consumer("MEMBERS"),
        KeyEntry::RequestKey => Owner::Consumer("RECOVERY_WRAP"),
        KeyEntry::VaultKey => Owner::Consumer("PERIOD_VAULT"),
        KeyEntry::DeviceKey => Owner::Consumer("DEVICE_KEY"),
    }
}
