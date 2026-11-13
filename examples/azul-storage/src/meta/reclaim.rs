//! The lifecycle of a drive's data objects (feature `encryption`; ARCH28 RF44 / AR5a-5): what may
//! take a data object out of the bucket, and on what proof.
//!
//! A data object (`data/xx/<id>`: a file's version, or a pack of small files) used to be deleted
//! at a dozen places on a dozen proofs, each proof a guess ("the index refused it", "the head does
//! not name it", "thirty days passed"). The rules of this module are few:
//!
//! - **A new object that no change named** is taken back out only by [`drop_new_object_after`], and
//!   only on a [`Refusal`]: the proof that the change was never made - the index's own check of
//!   its expectation said no ([`DriveError::Conflict`]), or the call was stopped before a byte went
//!   out ([`Certainty::NotAttempted`]) - made there of the error and the object and nowhere else.
//!   A change whose outcome is not known - no answer, a service error, and a refusal of policy
//!   (`Denied`, a node's `QuotaExceeded` or `ReadOnlyMoving`: the answer of the LAST request the
//!   transport sent, while the FIRST may still land after the index is read back, X-DELTA-SYNC7-1)
//!   - may have landed; the object it uploaded stays, written in the journal of what is in flight
//!   for the upkeep (garbage at worst, never a name without its object).
//! - **Nothing else** deletes a data object of a drive that keeps its history (the drive index is
//!   a repository: every state of the drive is a commit). Deleting is a decision about history,
//!   and only the reclaimer makes it.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    fmt,
};

use sha2::{Digest, Sha256};

use super::{
    bucket::Bucket,
    certainty::Certainty,
    hex,
    objects::{Mode, ObjectId as GitId, Objects, TreeEntry},
    pointer,
    repo::MetaRepo,
    retention::{Retention, RETENTION},
    seal::Sealer,
    shard,
    wal::{LeaseGuard, ReclaimMark, MAINTENANCE},
    MetaError,
};
use crate::{crypto::ObjectId, BatchDelete, Drive, DriveError};

/// The proof that the change which was to name a new object did not happen. Made from an error that
/// says so - a [`DriveError::Conflict`] (the index's own check of the change's expectation) or one whose
/// certainty is [`Certainty::NotAttempted`] - and the object the change was to name, and from nothing
/// else; it cannot be copied, defaulted or read from a file, and it is
/// for that object and no other ([`Refusal::covers`]). The type is public so that a reason to delete
/// ([`crate::erase::Why::Refused`]) can carry it; no one outside this module can make one: its fields
/// are private and its one constructor is private ([`drop_new_object_after`] is the door).
///
/// ```compile_fail,E0451
/// // Not made by hand: the fields are private.
/// use azul_storage::meta::reclaim::Refusal;
/// fn forge(id: azul_storage::crypto::ObjectId) -> Refusal {
///     Refusal { id, _sealed: () }
/// }
/// ```
///
/// ```compile_fail,E0624
/// // Not made from an error by a caller outside the module (nor by one inside the crate).
/// use azul_storage::{meta::reclaim::Refusal, DriveError};
/// fn forge(error: &DriveError, id: &azul_storage::crypto::ObjectId) -> Option<Refusal> {
///     Refusal::of_object(error, id)
/// }
/// ```
///
/// What compiles - the control: a refusal that was made is a reason.
///
/// ```no_run
/// use azul_storage::{erase::Why, meta::reclaim::Refusal};
/// fn reason(refusal: &Refusal) -> Why<'_> {
///     Why::Refused(refusal)
/// }
/// # let _ = reason;
/// ```
#[derive(Debug)]
pub struct Refusal {
    /// The new object the refused change was to name.
    id: ObjectId,
    _sealed: (),
}

impl Refusal {
    /// The refusal `error` is, if it is one, of the change that was to name the new object `id`;
    /// `None` for a change that may have landed. Only [`drop_new_object_after`] makes one: a refusal
    /// does not leave this module except inside a reason to delete.
    ///
    /// The ONE rule of what a compensation may take for "the change did not happen" (X-DELTA-SYNC7-1):
    ///
    /// - a [`DriveError::Conflict`]: [`NameIndex::apply`](crate::encrypted::NameIndex::apply) answers it
    ///   only when an expectation of the change did not hold against the state the index holds, and
    ///   nothing was sent for the change; an index never answers it for a conditional write that was
    ///   sent and lost (that is an unknown outcome, `Transport` or `Protocol`);
    /// - [`Certainty::NotAttempted`]: the call was refused before a byte went out.
    ///
    /// Not a refusal of POLICY - [`DriveError::Denied`], a node's `QuotaExceeded`, `ReadOnlyMoving`,
    /// `SessionExpired`, ...: [`DriveError::certainty`] calls those [`Certainty::Refused`], and that is
    /// the answer of the LAST request. A transport that sent a conditional write again after a lost
    /// answer hands on the refusal of the repeat while the first request is on its way to the bucket
    /// still; it lands after the index was read back, the manifest names the change, and the object
    /// this would delete is the one it names. Such an object stays and is journaled (ARCH30-5, A3 of
    /// ARCH29 9.4, mints `Refused` for a leaf's first and only attempt; until it lands a policy refusal
    /// is unknown here).
    fn of_object(error: &DriveError, id: &ObjectId) -> Option<Refusal> {
        let never_made = match error {
            DriveError::Conflict { .. } => true,
            other => other.certainty() == Certainty::NotAttempted,
        };
        never_made.then_some(Refusal {
            id: *id,
            _sealed: (),
        })
    }

    /// Whether this refusal is for the object at `key` of the bucket: the key of the object it was
    /// made for, and no other.
    pub(crate) fn covers(&self, key: &str) -> bool {
        self.id.bucket_key() == key
    }

    /// A refusal for the object `id`, for a test of what a reason covers.
    #[cfg(test)]
    pub(crate) fn lab(id: ObjectId) -> Refusal {
        Refusal { id, _sealed: () }
    }
}

/// Takes the new object `id` back out of `drive`'s bucket when `error`, the answer of the change that
/// was to name it, says the change was never made ([`Refusal::of_object`]: the index's own expectation
/// did not hold, or the call never went out): `Some` with what the bucket answered to the delete (a
/// delete it refuses leaves the object where it is - named by nothing, and the caller says so). `None`
/// for a change that may have landed, a refusal of policy included: nothing is deleted. The one door to
/// the compensation: the [`Refusal`] is made here, of the error and the object, and goes no further.
pub(crate) fn drop_new_object_after<D: Drive + ?Sized>(
    drive: &D,
    id: &ObjectId,
    error: &DriveError,
) -> Option<Result<(), DriveError>> {
    let refusal = Refusal::of_object(error, id)?;
    Some(drop_new_object(drive, id, &refusal))
}

/// Takes the new object `id` back out of `drive`'s bucket: the one compensation of a change that
/// did not name it (`_refusal` is the proof). A delete the bucket refuses leaves the object where
/// it is - named by nothing, and the caller says so.
///
/// # Errors
///
/// The bucket's error.
fn drop_new_object<D: Drive + ?Sized>(
    drive: &D,
    id: &ObjectId,
    _refusal: &Refusal,
) -> Result<(), DriveError> {
    drive.delete(&id.bucket_key())
}

/// What an index releases when a change leaves objects that no entry of its head names any more.
/// Two obligations that used to share one return value: an object nobody can ask for again is
/// deleted at once; one the history still names is not deleted by whoever released it.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Released {
    /// Objects no state of the drive names any more - an index that keeps no history
    /// ([`crate::encrypted::MemoryIndex`]): they may go from the bucket now.
    pub delete_now: Vec<ObjectId>,
    /// Objects the head no longer names but the history does (a repository: every state of the
    /// drive is a commit). They stay in the bucket until the reclaimer has proved them past their
    /// retention ([`super::retention::RETENTION`]). A hint: it tells a caller not to delete, and
    /// wakes the reclaimer; nothing depends on it, because the reclaimer derives what was released
    /// from the commits themselves.
    pub held_by_history: Vec<ObjectId>,
}

impl Released {
    /// Nothing released.
    #[must_use]
    pub fn none() -> Released {
        Released::default()
    }

    /// `ids` can go from the bucket now.
    #[must_use]
    pub fn delete_now(ids: Vec<ObjectId>) -> Released {
        Released {
            delete_now: ids,
            held_by_history: Vec::new(),
        }
    }

    /// `ids` are held by the history.
    #[must_use]
    pub fn held(ids: Vec<ObjectId>) -> Released {
        Released {
            delete_now: Vec::new(),
            held_by_history: ids,
        }
    }

    /// Whether nothing was released.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.delete_now.is_empty() && self.held_by_history.is_empty()
    }

    /// What `other` released as well.
    pub fn extend(&mut self, other: Released) {
        self.delete_now.extend(other.delete_now);
        self.held_by_history.extend(other.held_by_history);
    }
}

/// Deletes the objects an index without history released ([`Released::delete_now`]). The objects the
/// bucket would not delete come back with its answer: they are named by nothing and count against
/// the quota, so the caller keeps them to try again and says so (AZ-U22) - they used to be dropped
/// on the floor.
pub(crate) fn delete_released<D: Drive + ?Sized>(
    drive: &D,
    released: &Released,
) -> Vec<(ObjectId, DriveError)> {
    if released.delete_now.is_empty() {
        return Vec::new();
    }
    let keys: Vec<String> = released
        .delete_now
        .iter()
        .map(ObjectId::bucket_key)
        .collect();
    let items: Vec<BatchDelete<'_>> = keys.iter().map(|key| BatchDelete::new(key)).collect();
    let mut answers = drive.delete_many(&items).into_iter();
    let mut failed = Vec::new();
    for id in &released.delete_now {
        // A drive that answers for fewer items than it was asked is not taken to have deleted
        // the rest.
        let answer = answers.next().unwrap_or_else(|| {
            Err(DriveError::Protocol(String::from(
                "the storage answered for fewer deletes than were asked",
            )))
        });
        if let Err(e) = answer {
            failed.push((*id, e));
        }
    }
    failed
}

// ==== The reclaimer ====
//
// What a repository keeps is every state of the drive. A data object that the head no longer names
// is still named by the commits that did - and a user can ask for those states back for
// [`Retention::versions`]. When the last commit that names an object is older than that, nobody
// can ask for it any more, and the object only costs quota: it is reclaimable. The reclaimer
// derives this from the commits themselves; it records nothing a lost write could falsify, and
// nothing an older client does can make it delete more.
//
// Two leaks, never deletions (they keep an object, they do not lose one): a commit that arrives after
// the mark has passed its time - a device that was offline and publishes an old state - is older than
// the mark and is never read as releasing anything, so what it released stays; and a commit stamped in
// the future keeps itself and its descendants inside the window until the real time has passed it and
// the retention (the cap is against the clock of the run, so it never ages). The docs of the retention
// promise "ninety days"; for these two the objects stay longer.

/// Whether and how the reclaimer runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReclaimMode {
    /// It does not run: the kill switch.
    Off,
    /// It runs, says "would delete N objects, M bytes" and deletes nothing: how it ships first.
    Shadow,
    /// It deletes what it proved.
    Delete,
}

/// The mode a drive's reclaimer starts in. The first release ships it in [`ReclaimMode::Shadow`]
/// (ARCH28 Q30: the most dangerous code of the client runs a release without a delete); the
/// next release changes this one line.
pub const DEFAULT_MODE: ReclaimMode = ReclaimMode::Shadow;

/// How much one run may do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Caps {
    /// The commits older than the window whose trees a run reads.
    pub commits: usize,
    /// The objects a run decides on.
    pub objects: usize,
    /// The bytes of the objects a run decides on: the first round that deletes takes at most this
    /// much off a drive, whatever the count (the scan ends at the commit that passes it).
    pub candidate_bytes: u64,
    /// The objects deleted between two fences.
    pub batch: usize,
    /// The bytes of the repository's packs a run reads, as the manifest says them before anything
    /// is fetched. A run holds every object it reads in memory, so a drive whose index is bigger
    /// than this is too big to judge ([`NotYet::TooBig`]): it is not read, and nothing is decided
    /// from it (X-DELTA-AZDRIVE-SYNC-5).
    pub pack_bytes: u64,
}

/// The packs' bytes a reclaimer run reads at most ([`Caps::pack_bytes`]): the objects of a pack are
/// held inflated in memory, a few times its size, and the machines this runs on have 8 GiB.
pub const DEFAULT_PACK_BYTES: u64 = 256 * 1024 * 1024;

/// The bytes of objects a reclaimer run decides on at most ([`Caps::candidate_bytes`]).
pub const DEFAULT_CANDIDATE_BYTES: u64 = 20 * 1024 * 1024 * 1024;

impl Default for Caps {
    fn default() -> Self {
        Caps {
            commits: 250_000,
            objects: 50_000,
            candidate_bytes: DEFAULT_CANDIDATE_BYTES,
            batch: 1_000,
            pack_bytes: DEFAULT_PACK_BYTES,
        }
    }
}

/// Whether the bucket's clock must agree with this device's before anything is deleted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ServerClock {
    /// The bucket says when the lease the run took was written, and that time and this device's
    /// agree within [`SKEW_SECS`]; a bucket that does not say, or a device whose clock is off,
    /// deletes nothing. The age of a commit is the device's word otherwise.
    Required,
    /// The device's clock alone (a folder on disk, the tests).
    Unchecked,
}

/// What a reclaimer run is told.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReclaimOptions {
    pub mode: ReclaimMode,
    pub caps: Caps,
    pub retention: Retention,
    pub server_clock: ServerClock,
    /// Whether the run goes on from the manifest's mark and moves it. A purge ([`purge_options`])
    /// reads the whole history and leaves the mark where it is.
    pub use_mark: bool,
    /// A purge the user confirmed at this time (seconds since 1970, the confirmation's own record:
    /// [`PurgeConfirmation::at`]): the versions the history keeps are the ones FIRST named by a commit
    /// made after it, and `retention` is not read ([`purge_options`]). `None` for the reclaimer's own run.
    pub purge_after: Option<u64>,
}

impl ReclaimOptions {
    /// The reclaimer as the first release ships it (ARCH28 Q30): it runs, says what it would
    /// delete and deletes nothing. There is no `Default`: an app that builds an index provider says
    /// which it wants ([`ReclaimOptions::shadow`] or [`ReclaimOptions::off`]), and a reader of its
    /// code sees it (X-DELTA-AZDRIVE-SYNC-3).
    #[must_use]
    pub fn shadow() -> ReclaimOptions {
        ReclaimOptions {
            mode: DEFAULT_MODE,
            caps: Caps::default(),
            retention: super::retention::RETENTION,
            server_clock: ServerClock::Required,
            use_mark: true,
            purge_after: None,
        }
    }

    /// The kill switch: the reclaimer does not run.
    #[must_use]
    pub fn off() -> ReclaimOptions {
        ReclaimOptions {
            mode: ReclaimMode::Off,
            ..ReclaimOptions::shadow()
        }
    }
}

// ==== The switch to deleting ====
//
// What the first release ships is the reclaimer in shadow mode: it reports what it would delete and
// deletes nothing. The release after it does not flip a constant: a run that is configured to delete
// deletes only when [`go`] says so, from what the shadow rounds of THIS drive recorded - rounds
// that ran to their end, whose live set an independent listing of the head agreed with, over days and
// on several devices (ARCH29 C8, X-DELTA-AZDRIVE-SYNC-6). A drive nobody has watched is not deleted
// from, whatever the mode says.

/// How many shadow rounds in a row (any device) have to be clean before a run may delete.
pub const GO_CLEAN_ROUNDS: u32 = 7;

/// How many days the clean rounds have to span.
pub const GO_DAYS: u32 = 14;

/// How many devices have to have shown it - fewer when the drive has fewer devices.
pub const GO_DEVICES: u32 = 3;

/// What the shadow rounds of a drive have shown, and what this run knows about its own footing.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct GoEvidence {
    /// The clean shadow rounds in a row at the end of the record (any device).
    pub clean_rounds: u32,
    /// Whole days from the first of them to the last.
    pub days: u32,
    /// The devices that ran at least one of them.
    pub devices: u32,
    /// The devices that committed to the drive inside the retention window (at least one).
    pub drive_devices: u32,
    /// What the independent listing of the head said of this round's live set: `Some(true)` it
    /// names nothing the live set lacks, `Some(false)` it does, `None` it was not made (or was made
    /// at another revision than the analysis).
    pub rederived_live_equal: Option<bool>,
    /// What the restore dialog still offers (derived its own way: [`restorable`]) is named by the
    /// run's window: `Some(true)` it is, `Some(false)` it is not, `None` it was not derived. The
    /// listing of the head cannot see a mistake of the WINDOW; this can (X-DELTA-SYNC2-2).
    pub window_rederived: Option<bool>,
    /// This device's clock agrees with the bucket's at the round that made the evidence:
    /// `Some(true)` the bucket told its time and it is within [`SKEW_SECS`] of ours (or the run's
    /// policy takes the device's clock alone), `Some(false)` it is not, `None` the bucket does not
    /// say. The age of every commit, and the days of the clean rounds, rest on it.
    pub clock_agrees: Option<bool>,
    /// The drive's index fits the budget of a run ([`Caps::pack_bytes`]).
    pub byte_budget_ok: bool,
    /// This device's copy of the drive knows the highest revision it has seen: a rolled-back
    /// bucket is refused, not read as the drive.
    pub floor_known: bool,
    /// The app knows where a restore of the drive is announced.
    pub restore_source: bool,
}

/// Why a run that is configured to delete only reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Hold {
    /// The run is configured as a shadow run.
    Configured,
    /// Fewer clean rounds in a row than [`GO_CLEAN_ROUNDS`].
    Rounds { have: u32, need: u32 },
    /// The clean rounds span fewer days than [`GO_DAYS`].
    Days { have: u32, need: u32 },
    /// Fewer devices showed it than the drive's devices allow ([`GO_DEVICES`] at most).
    Devices { have: u32, need: u32 },
    /// The independent listing of the head was not made.
    NotRederived,
    /// The independent listing of the head names an object the live set lacks.
    Differs,
    /// The drive's index is over the budget of a run.
    TooBig,
    /// This device's copy knows no revision to refuse a rollback by.
    NoFloor,
    /// The app knows no restore source.
    NoRestoreSource,
    /// The reclaimer is switched off.
    Off,
    /// No round of this drive was judged by this process: nothing shows what its shadow rounds
    /// came to (X-DELTA-SYNC2-2: the pack upkeep deleted with none).
    NotJudged,
    /// The last judged round is older than the rounds' schedule allows (or stamped in the future):
    /// what it showed is not what the drive shows now.
    Stale,
    /// What the restore dialog still offers was not derived again beside the analysis's window.
    WindowNotRederived,
    /// What the restore dialog still offers names an object the analysis's window does not: the
    /// window of the run is not the one the user was promised.
    WindowDiffers,
    /// The bucket's clock was not asked, or the bucket does not say the time.
    ClockNotChecked,
    /// This device's clock and the bucket's disagree: the round's own time, and the age of every
    /// commit it judged, are not believed.
    ClockDiffers,
    /// The drive's kill switch is set ([`keys::RECLAIM_HOLD`](super::keys::RECLAIM_HOLD)): nothing it keeps is
    /// deleted while it is.
    KillSwitch,
    /// Whether the drive's kill switch is set could not be read: a deleter that cannot read it does not
    /// delete.
    SwitchNotRead,
}

impl fmt::Display for Hold {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Hold::Configured => f.write_str("configured as a shadow run"),
            Hold::Rounds { have, need } => {
                write!(f, "{have} clean rounds in a row, {need} wanted")
            }
            Hold::Days { have, need } => write!(f, "clean for {have} days, {need} wanted"),
            Hold::Devices { have, need } => write!(f, "shown by {have} devices, {need} wanted"),
            Hold::NotRederived => {
                f.write_str("the head was not listed again to check the live set")
            }
            Hold::Differs => f.write_str("the listing of the head names what the live set lacks"),
            Hold::TooBig => f.write_str("the drive's index is over the budget of a run"),
            Hold::NoFloor => f.write_str("this device's copy has seen no revision of the drive"),
            Hold::NoRestoreSource => f.write_str("the app knows no restore announcement"),
            Hold::Off => f.write_str("the reclaimer is switched off"),
            Hold::NotJudged => f.write_str("no shadow round of the drive was judged lately"),
            Hold::Stale => {
                f.write_str("the last judged shadow round of the drive is too old to be trusted")
            }
            Hold::WindowNotRederived => f.write_str(
                "what the restore dialog still offers was not derived again beside the window",
            ),
            Hold::WindowDiffers => f.write_str(
                "the restore dialog still offers what the window of the run does not name",
            ),
            Hold::ClockNotChecked => {
                f.write_str("the storage does not tell the time to check this device's clock by")
            }
            Hold::ClockDiffers => {
                f.write_str("this device's clock and the storage's disagree about the time of day")
            }
            Hold::KillSwitch => f.write_str("the drive's kill switch for the deleting is set"),
            Hold::SwitchNotRead => {
                f.write_str("the drive's kill switch for the deleting could not be read")
            }
        }
    }
}

/// What a run configured as `mode` does.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Go {
    /// Deletes what it proved.
    Delete,
    /// Reports what it would delete, and why it does not.
    Shadow(Hold),
    /// Does not run.
    Off,
}

/// Whether a run configured as `mode` deletes, from the evidence `e`: pure and total. The table of
/// the evidence, and the one rule [`ask_retired_delete`] applies to it (with the standing's age):
/// the kill switch is `Off`, a shadow run is a shadow run, and a run configured to delete is held
/// back by the first thing the evidence lacks, in the order of the table in the tests.
#[must_use]
pub fn go(mode: ReclaimMode, e: &GoEvidence) -> Go {
    match mode {
        ReclaimMode::Off => Go::Off,
        ReclaimMode::Shadow => Go::Shadow(Hold::Configured),
        ReclaimMode::Delete => {
            if !e.restore_source {
                return Go::Shadow(Hold::NoRestoreSource);
            }
            if !e.floor_known {
                return Go::Shadow(Hold::NoFloor);
            }
            if !e.byte_budget_ok {
                return Go::Shadow(Hold::TooBig);
            }
            match e.rederived_live_equal {
                Some(true) => {}
                Some(false) => return Go::Shadow(Hold::Differs),
                None => return Go::Shadow(Hold::NotRederived),
            }
            match e.window_rederived {
                Some(true) => {}
                Some(false) => return Go::Shadow(Hold::WindowDiffers),
                None => return Go::Shadow(Hold::WindowNotRederived),
            }
            match e.clock_agrees {
                Some(true) => {}
                Some(false) => return Go::Shadow(Hold::ClockDiffers),
                None => return Go::Shadow(Hold::ClockNotChecked),
            }
            if e.clean_rounds < GO_CLEAN_ROUNDS {
                return Go::Shadow(Hold::Rounds {
                    have: e.clean_rounds,
                    need: GO_CLEAN_ROUNDS,
                });
            }
            if e.days < GO_DAYS {
                return Go::Shadow(Hold::Days {
                    have: e.days,
                    need: GO_DAYS,
                });
            }
            let need = GO_DEVICES.min(e.drive_devices.max(1));
            if e.devices < need {
                return Go::Shadow(Hold::Devices {
                    have: e.devices,
                    need,
                });
            }
            Go::Delete
        }
    }
}

// ==== The one gate of every deleter of data the history may still name ====
//
// Four things take a data object out of the bucket: the reclaimer's own delete run, the pack upkeep
// (the retired packs past their grace, the objects whose writer stopped), the objects an index that
// keeps no history released, and a user's confirmed purge. Each used to decide for itself whether it
// may - the run asked [`go`], the upkeep took the configured mode - and the one that did not ask
// deleted whatever the evidence said (X-DELTA-SYNC2-2). Now `ask_retired_delete` is the ONE function
// that answers, and the deleters take its answer: [`delete_reclaimable`] and [`remove_reclaimable`]
// delete only for a [`DeleteLicence`], which only this module makes and only from that answer (or the
// two documented others: an index with no history, a purge the user confirmed). A source gate
// (`azul-srcgate/tests/retired_delete_gate.rs`) finds every function that deletes a data object and
// every place a licence is made.

/// What the last judged round of a drive came to, and when: the standing the pack upkeep and the
/// reclaimer's own run ask [`ask_retired_delete`] with.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Standing {
    /// When the round was judged (seconds since 1970, this device's clock).
    pub at: u64,
    /// What the ledger of the drive's shadow rounds and the round's own footing showed.
    pub evidence: GoEvidence,
}

/// How long a [`Standing`] lets the pack upkeep delete (seconds): rounds are a day apart
/// ([`super::shadow::EVERY_SECS`]), and a process that has not judged the drive for two days has no
/// standing to ask with - the upkeep then reports and deletes nothing.
pub const STANDING_FRESH_SECS: u64 = 2 * super::shadow::EVERY_SECS;

/// What the drive's kill switch says ([`keys::RECLAIM_HOLD`](super::keys::RECLAIM_HOLD)): an object that is
/// there or not. Read by every deleter right before it asks the one gate - the reclaimer's round, each batch of
/// its delete run, the pack upkeep - and never remembered: a hold takes effect at the next delete, not at the
/// next round (X-DELTA-AZDRIVE-SYNC-6). What cannot be read is its own answer, and the gate holds on it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Switch {
    /// No hold: the object is not there.
    Clear,
    /// The hold is there.
    Held,
    /// The bucket could not say whether it is there.
    Unread,
}

impl Switch {
    /// The switch as a look for the object said: `Some(true)` it is there, `Some(false)` it is not, `None` the
    /// look failed. The one place that reads a look.
    fn of_presence(there: Option<bool>) -> Switch {
        match there {
            Some(false) => Switch::Clear,
            Some(true) => Switch::Held,
            None => Switch::Unread,
        }
    }

    /// The switch of the drive in `drive` (one HEAD).
    #[must_use]
    pub fn read_drive<D: Drive + ?Sized>(drive: &D) -> Switch {
        Switch::of_presence(match drive.head(super::keys::RECLAIM_HOLD) {
            Ok(_) => Some(true),
            Err(DriveError::NotFound { .. }) => Some(false),
            Err(_) => None,
        })
    }

    /// The switch of the repository in `bucket`.
    #[must_use]
    pub fn read_bucket<B: Bucket + ?Sized>(bucket: &B) -> Switch {
        Switch::of_presence(bucket.exists(super::keys::RECLAIM_HOLD).ok())
    }
}

/// What the index of a drive keeps, as far as a deleter's question goes.
#[derive(Clone, Copy, Debug)]
pub enum History<'a> {
    /// The index keeps no history (an index in memory): what the head does not name is named by
    /// nobody, and nothing is retired.
    None,
    /// The index keeps its history: data objects only it names are deleted in the mode the drive is
    /// configured with, and only with the evidence of its judged shadow rounds.
    Kept {
        mode: ReclaimMode,
        standing: Option<&'a Standing>,
        /// The drive's kill switch as the deleter read it just now ([`Switch`]).
        switch: Switch,
    },
}

/// The licence to delete data objects the history may still name: made by [`ask_retired_delete`]
/// (from the evidence of a drive's shadow rounds), by [`DeleteLicence::of_an_index_without_history`]
/// and by [`DeleteLicence::confirmed`] (a purge the user confirmed), and by nothing else - no field
/// is public, it cannot be copied, defaulted or read from a file. It says WHEN the history lets a
/// data object go, not which: the object is named by the proof beside it ([`Reclaimable`]), and a
/// reason to delete ([`crate::erase::Why::Retired`]) is the two together.
///
/// ```compile_fail,E0451
/// // Not made by hand: the field is private.
/// use azul_storage::meta::reclaim::DeleteLicence;
/// let _licence = DeleteLicence { _sealed: () };
/// ```
///
/// What compiles - the control: a licence somebody asked for, with the proof, is a reason.
///
/// ```no_run
/// use azul_storage::{erase::Why, meta::reclaim::{DeleteLicence, Reclaimable}};
/// fn reason<'a>(licence: &'a DeleteLicence, proof: &'a Reclaimable) -> Why<'a> {
///     Why::Retired(licence, proof)
/// }
/// # let _ = reason;
/// ```
#[derive(Debug)]
pub struct DeleteLicence {
    _sealed: (),
}

/// A deleter's answer from [`ask_retired_delete`]: the licence, or what holds the delete back.
pub type Gate = Result<DeleteLicence, Hold>;

impl DeleteLicence {
    /// The licence of a drive whose index keeps no history ([`History::None`]).
    fn of_an_index_without_history() -> DeleteLicence {
        DeleteLicence { _sealed: () }
    }

    /// The licence of a purge the user confirmed ([`PurgeConfirmation`]): the user said so, the
    /// shadow rounds are not asked.
    pub(crate) fn confirmed(_confirmation: &PurgeConfirmation) -> DeleteLicence {
        DeleteLicence { _sealed: () }
    }

    /// A licence for a test of the deleting machinery itself (the fences, the batches, the lease):
    /// what the tests of the gate say about WHEN one is made is in `ask_retired_delete`'s own.
    #[cfg(test)]
    pub(crate) fn lab() -> DeleteLicence {
        DeleteLicence { _sealed: () }
    }
}

/// THE question every deleter of data the history may still name asks before it deletes, at `now`
/// (the clock of this device): the licence, or why not.
///
/// - an index that keeps no history ([`History::None`]) has no window: the licence;
/// - a drive configured as a shadow run or switched off never deletes;
/// - a drive configured to delete deletes only while the standing of its last judged round is fresh
///   ([`STANDING_FRESH_SECS`]; none, or one stamped in the future, is [`Hold::NotJudged`] or
///   [`Hold::Stale`]) and [`go`] says [`Go::Delete`] of the evidence in it: the clean rounds, the
///   days, the devices and the footing the switch to deleting promises.
///
/// The reclaimer's own run asks it with the standing of the round it has just judged, the pack upkeep
/// with the standing its provider recorded of the last one: one rule, not a copy of it.
///
/// # Errors
///
/// What holds the delete back.
pub(crate) fn ask_retired_delete(history: History<'_>, now: u64) -> Gate {
    let (mode, standing, switch) = match history {
        History::None => return Ok(DeleteLicence::of_an_index_without_history()),
        History::Kept {
            mode,
            standing,
            switch,
        } => (mode, standing, switch),
    };
    match mode {
        ReclaimMode::Off => return Err(Hold::Off),
        ReclaimMode::Shadow => return Err(Hold::Configured),
        ReclaimMode::Delete => {}
    }
    // The kill switch outranks every piece of evidence: a hold, or a switch nobody could read, is no delete.
    match switch {
        Switch::Clear => {}
        Switch::Held => return Err(Hold::KillSwitch),
        Switch::Unread => return Err(Hold::SwitchNotRead),
    }
    let Some(standing) = standing else {
        return Err(Hold::NotJudged);
    };
    if standing.at > now || now - standing.at > STANDING_FRESH_SECS {
        return Err(Hold::Stale);
    }
    match go(mode, &standing.evidence) {
        Go::Delete => Ok(DeleteLicence { _sealed: () }),
        Go::Shadow(hold) => Err(hold),
        Go::Off => Err(Hold::Off),
    }
}

/// What the user confirmed when they asked for the old versions of a drive to be erased: after a
/// key was changed ("I was hacked": re-encrypting every file leaves the old objects, which the old
/// key still opens, in the history) the history is restorable until the retention is over -
/// unless they say it is not. Made from the answer an app recorded in the re-encryption's state
/// ([`PurgeConfirmation::recorded_in`], after [`crate::rotation::ReencryptState::confirm_purge`]) and
/// from nothing else: the constructor from a number is this module's, and nothing outside it can
/// call it. What no type can say is that the user really answered. The app records it, through
/// `confirm_purge`, whose callers are the commands and dialogs of a user's own request and are
/// listed by a text gate (`azul-srcgate/tests/confirm_purge_gate.rs`): the door is that record, and
/// a state that was saved with an answer in it is the same record read back (X-DELTA-KEYS4-5).
///
/// ```compile_fail,E0451
/// // Not made by hand: the field is private.
/// use azul_storage::meta::reclaim::PurgeConfirmation;
/// let _confirmation = PurgeConfirmation { at: 1, _sealed: () };
/// ```
///
/// ```compile_fail,E0624
/// // Not made from a number DIRECTLY: the constructor is private. (A number recorded with
/// // `ReencryptState::confirm_purge` and read back with `purge_confirmation` is the other door,
/// // the one the gate above watches.)
/// use azul_storage::meta::reclaim::PurgeConfirmation;
/// let _confirmation = PurgeConfirmation::confirmed_by_the_user(1);
/// ```
///
/// What compiles - the control: the confirmation a pass recorded is read, and is a reason.
///
/// ```no_run
/// use azul_storage::{erase::Why, meta::reclaim::{PurgeConfirmation, Reclaimable}};
/// fn reason<'a>(confirmation: &'a PurgeConfirmation, proof: &'a Reclaimable) -> Why<'a> {
///     Why::Confirmed(confirmation, proof)
/// }
/// fn when(confirmation: &PurgeConfirmation) -> u64 {
///     confirmation.at
/// }
/// # let _ = (reason, when);
/// ```
// The private field and not `#[non_exhaustive]`: only this module can make one, where
// `non_exhaustive` would let the whole crate.
#[allow(clippy::manual_non_exhaustive)]
#[derive(Debug)]
pub struct PurgeConfirmation {
    /// When they confirmed (seconds since 1970).
    pub at: u64,
    _sealed: (),
}

impl PurgeConfirmation {
    /// The confirmation the user gave that `state` records, if they gave one: the one door from a
    /// recorded answer ([`crate::rotation::ReencryptState::confirm_purge`], saved with the pass so
    /// that a restart does not lose it) to a confirmation. The constructor below is this module's
    /// and nothing outside it makes one from a number.
    #[must_use]
    pub(crate) fn recorded_in(
        state: &crate::rotation::ReencryptState,
    ) -> Option<PurgeConfirmation> {
        state
            .purge_confirmed_at
            .map(PurgeConfirmation::confirmed_by_the_user)
    }

    /// The user confirmed at `at` that every old version of every file is erased now, for good:
    /// the drive's history before this moment cannot be restored afterwards. Only
    /// [`PurgeConfirmation::recorded_in`] calls it.
    fn confirmed_by_the_user(at: u64) -> PurgeConfirmation {
        PurgeConfirmation { at, _sealed: () }
    }

    /// A confirmation at `at`, for a test of what a purge does with one.
    #[cfg(test)]
    pub(crate) fn lab(at: u64) -> PurgeConfirmation {
        PurgeConfirmation::confirmed_by_the_user(at)
    }
}

/// The options of a purge: every object the head does not name that the history released and that
/// was named by a commit made at or before the moment the user confirmed, whatever its age, deleted;
/// the whole history read; no mark kept. The bucket's clock must still agree with the device's.
///
/// What the user confirmed is the erasure of the versions there were when they said so - the old
/// objects the old key opens, which a re-encryption pass replaces one by one in the hours after the
/// click. Those go however late the commit that replaced them is. What a commit made AFTER the
/// confirmation first names stays (a version the user never agreed to lose), and so does what the
/// head names. The boundary is the confirmation's own time, carried as it is
/// ([`ReclaimOptions::purge_after`]) - not a span measured from a clock read before the repository
/// was opened (X-DELTA-SYNC6-1, which replaced X-DELTA-SYNC4-10's `now - at` window: that one kept
/// every object the click-time state named, i.e. none of the ones the purge was for).
#[must_use]
pub fn purge_options(confirmation: &PurgeConfirmation) -> ReclaimOptions {
    ReclaimOptions {
        mode: ReclaimMode::Delete,
        caps: Caps {
            commits: usize::MAX,
            objects: usize::MAX,
            candidate_bytes: u64::MAX,
            batch: Caps::default().batch,
            // The user asked for the whole history to be read.
            pack_bytes: u64::MAX,
        },
        // Not read: `purge_after` decides what is kept.
        retention: Retention {
            trash: std::time::Duration::ZERO,
            node: std::time::Duration::ZERO,
            versions: std::time::Duration::ZERO,
        },
        server_clock: ServerClock::Required,
        use_mark: false,
        purge_after: Some(confirmation.at),
    }
}

/// How far this device's clock and the bucket's may differ before the age of a commit is not
/// believed (seconds).
pub const SKEW_SECS: u64 = 600;

/// Whether the bucket's time `server` agrees with the time of day a decision was made at: `stamp`, the
/// device's clock when the analysis began (the window it cut and the stamp of its round), carried
/// forward by `elapsed_secs` of the MONOTONIC clock since. The device's clock of the moment the bucket
/// is asked says nothing about the stamp's: a clock that was set wrong when the analysis began and is
/// corrected before it ends would agree with the bucket at the end and have cut the window days off
/// (X-DELTA-SYNC4-4). A suspended machine's monotonic clock stands still, so a long suspension makes the
/// stamp and the bucket disagree - a round that does not count, never one that does.
#[must_use]
pub(crate) fn stamp_agrees(server: u64, stamp: u64, elapsed_secs: u64) -> bool {
    server.abs_diff(stamp.saturating_add(elapsed_secs)) <= SKEW_SECS
}

/// A commit stamped further ahead of the device's clock than this counts as stamped then: one
/// device with a wrong clock must not make a whole history look younger than it is.
const AHEAD_SECS: u64 = 86_400;

/// How long a run keeps the maintenance lease between two renewals (seconds).
const LEASE_SECS: u64 = 900;

/// Why a decision was not made: the reclaimer says "not yet" and deletes nothing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NotYet {
    /// The kill switch is on.
    Off,
    /// A tree or a pointer could not be read: nothing is decided from what cannot be read.
    Unreadable { what: String, reason: String },
    /// The head or a commit of the window names the object.
    Named,
    /// The history does not show the object released by a commit past its retention.
    NotReleased,
    /// The drive moved since it was read: the reads are old.
    Moved,
    /// This device's clock and the bucket's disagree (or the bucket does not say).
    Clock { device: u64, server: Option<u64> },
    /// Another device holds the maintenance lease, or this one lost it.
    Lease,
    /// The drive's index is bigger than a run reads ([`Caps::pack_bytes`]): nothing was fetched.
    TooBig { bytes: u64, budget: u64 },
    /// A purge's confirmation is dated after this device's clock by more than [`SKEW_SECS`]: what it
    /// covers cannot be told (a clock that was set back since), and nothing is erased on a guess.
    Confirmation { at: u64, now: u64 },
    /// The drive's kill switch was set, or could not be read, before a batch of the delete run
    /// ([`Switch`]): the run stops there and deletes no more.
    Switch(Switch),
}

impl fmt::Display for NotYet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NotYet::Off => f.write_str("the reclaimer is switched off"),
            NotYet::Unreadable { what, reason } => {
                write!(
                    f,
                    "\"{what}\" cannot be read ({reason}): nothing is decided"
                )
            }
            NotYet::Named => f.write_str("the head or a version inside the window names it"),
            NotYet::NotReleased => f.write_str("no commit past the retention released it"),
            NotYet::Moved => f.write_str("the drive changed since it was read"),
            NotYet::Clock { device, server } => match server {
                Some(server) => write!(
                    f,
                    "this device's clock ({device}) and the storage's ({server}) disagree"
                ),
                None => write!(
                    f,
                    "the storage does not tell the time, and this device's clock ({device}) is \
                     not believed alone"
                ),
            },
            NotYet::Lease => f.write_str("another device is tidying the drive"),
            NotYet::TooBig { bytes, budget } => write!(
                f,
                "the drive's index is {} MiB and a run judges up to {} MiB: too big to judge",
                bytes / (1024 * 1024),
                budget / (1024 * 1024)
            ),
            NotYet::Confirmation { at, now } => write!(
                f,
                "the confirmation is dated {at}, after this device's clock ({now}): what it covers \
                 cannot be told, nothing is erased"
            ),
            NotYet::Switch(Switch::Held) => fmt::Display::fmt(&Hold::KillSwitch, f),
            NotYet::Switch(_) => fmt::Display::fmt(&Hold::SwitchNotRead, f),
        }
    }
}

impl NotYet {
    fn unreadable(what: impl Into<String>, error: impl fmt::Display) -> NotYet {
        NotYet::Unreadable {
            what: what.into(),
            reason: error.to_string(),
        }
    }
}

/// The proof that a data object may leave the bucket: the history released it, the last commit
/// that named it is past the retention, neither the head nor a commit of the window names it, and
/// it was read fresh and whole. Made by the reclaimer's analysis and by nothing else; it cannot
/// be copied, defaulted or read from a file. It is for ONE object: a reason to delete that carries
/// it ([`crate::erase::Why::Retired`]) is for this object's bucket key and no other.
///
/// ```compile_fail,E0451
/// // Not made by hand: the fields are private.
/// use azul_storage::meta::reclaim::Reclaimable;
/// fn forge(id: azul_storage::crypto::ObjectId) -> Reclaimable {
///     Reclaimable { id, _sealed: () }
/// }
/// ```
///
/// What compiles - the control: the proof is read, and says which object it is for.
///
/// ```no_run
/// use azul_storage::meta::reclaim::Reclaimable;
/// fn object(proof: &Reclaimable) -> azul_storage::crypto::ObjectId {
///     proof.id()
/// }
/// # let _ = object;
/// ```
#[derive(Debug)]
pub struct Reclaimable {
    id: ObjectId,
    _sealed: (),
}

impl Reclaimable {
    /// The object.
    #[must_use]
    pub fn id(&self) -> ObjectId {
        self.id
    }

    /// Whether this proof is for the object at `key` of the bucket: the key of its object, and no
    /// other (what a reason to delete that carries it covers, [`crate::erase::Why::Retired`]).
    pub(crate) fn covers(&self, key: &str) -> bool {
        self.id.bucket_key() == key
    }

    /// A proof for the object `id`, for a test of what a reason covers.
    #[cfg(test)]
    pub(crate) fn lab(id: ObjectId) -> Reclaimable {
        Reclaimable { id, _sealed: () }
    }
}

/// What became of a [`Reclaimable`] handed to the deleter.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Deleted {
    /// The object is gone from the bucket.
    Gone,
    /// Shadow mode (or off): it would have been, and nothing was deleted.
    Would,
}

/// The ONE delete of a data object that is not the compensation of a refused change: `proof` says
/// the history released it and its retention is over, and `gate` is the answer of
/// [`ask_retired_delete`]: with a licence the object is deleted, with what holds it back nothing is
/// (`Deleted::Would`). A caller that did not ask has no gate to give.
///
/// # Errors
///
/// The bucket's.
pub(crate) fn delete_reclaimable<D: Drive + ?Sized>(
    drive: &D,
    gate: &Gate,
    proof: Reclaimable,
) -> Result<Deleted, DriveError> {
    match gate {
        Ok(_licence) => drive.delete(&proof.id.bucket_key()).map(|()| Deleted::Gone),
        Err(_held) => Ok(Deleted::Would),
    }
}

/// [`delete_reclaimable`] for the repository's own bucket.
fn remove_reclaimable<B: Bucket>(
    bucket: &B,
    gate: &Gate,
    proof: Reclaimable,
) -> Result<Deleted, MetaError> {
    match gate {
        Ok(_licence) => bucket
            .remove(&proof.id.bucket_key())
            .map(|()| Deleted::Gone),
        Err(_held) => Ok(Deleted::Would),
    }
}

/// How far the analysis got through the commits older than the window: they are processed in the
/// order of this pair (effective time, then id).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub(crate) struct Mark {
    pub(crate) time: u64,
    pub(crate) id: GitId,
}

impl Mark {
    fn from_wal(mark: &ReclaimMark) -> Option<Mark> {
        Some(Mark {
            time: mark.time,
            id: GitId::from_hex(&mark.commit)?,
        })
    }

    fn to_wal(self, at: u64) -> ReclaimMark {
        ReclaimMark {
            time: self.time,
            commit: self.id.to_hex(),
            at,
        }
    }
}

/// An object an old commit released - it was in the tree of a parent and is not in its own - and
/// nothing in the window names.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Candidate {
    /// Its bytes in the bucket (a file's object, or a whole pack).
    pub(crate) bytes: u64,
    /// The first old commit that released it.
    pub(crate) by: GitId,
    /// That commit's effective time.
    pub(crate) at: u64,
}

/// What the history says of the data objects of one drive at one moment.
#[derive(Debug)]
pub(crate) struct Analysis {
    /// The manifest revision the reads were made at (set by whoever read).
    pub(crate) revision: u64,
    /// The time of day the ages were judged by.
    pub(crate) now: u64,
    /// When the analysis began by the monotonic clock: what carries [`Analysis::now`] forward to the
    /// moment the bucket is asked ([`stamp_agrees`]).
    pub(crate) started: crate::time::Stopwatch,
    /// The commits before this time are old (seconds since 1970).
    pub(crate) window_start: u64,
    /// The commits reachable from the head.
    pub(crate) commits: usize,
    /// Those inside the window.
    pub(crate) window_commits: usize,
    /// Every data object the head, a commit of the window or the state the window began with
    /// names, with its bytes.
    pub(crate) live: HashMap<ObjectId, u64>,
    /// The ones the head names alone: what a restore of an old version has already brought back.
    pub(crate) head: HashSet<ObjectId>,
    /// Objects old commits released and nothing live names.
    pub(crate) candidates: BTreeMap<ObjectId, Candidate>,
    /// The old commits whose trees were read this time.
    pub(crate) scanned: usize,
    /// The newest old commit dealt with.
    pub(crate) cursor: Option<Mark>,
    /// A cap ended the scan before the old commits did.
    pub(crate) stopped_by_cap: bool,
    /// The devices that committed inside the window (distinct committers).
    pub(crate) devices: usize,
    /// Whether everything a restore could still bring back ([`restorable`]) is in `live`
    /// (set by whoever read: it needs the repository's objects at the head).
    pub(crate) window_agrees: Option<bool>,
}

/// One commit of the graph.
struct Node {
    tree: GitId,
    parents: Vec<GitId>,
    /// The effective time: never earlier than a parent's, never further ahead than the device's
    /// clock by more than [`AHEAD_SECS`].
    eff: u64,
    /// The committer, as the number the graph gave its name.
    device: usize,
}

/// The commits reachable from `head`, parents before children, with their effective times.
fn load_graph(
    objects: &Objects,
    head: &GitId,
    now: u64,
) -> Result<(HashMap<GitId, Node>, Vec<GitId>), NotYet> {
    let ceiling = now.saturating_add(AHEAD_SECS);
    let mut nodes: HashMap<GitId, Node> = HashMap::new();
    let mut order: Vec<GitId> = Vec::new();
    // The committers, each named by the number it was given when it first appeared.
    let mut devices: HashMap<String, usize> = HashMap::new();
    // The commits read and waiting for their parents: (tree, parents, own time, committer).
    let mut pending: HashMap<GitId, (GitId, Vec<GitId>, u64, usize)> = HashMap::new();
    let mut stack: Vec<(GitId, bool)> = vec![(*head, false)];
    while let Some((id, expanded)) = stack.pop() {
        if nodes.contains_key(&id) {
            continue;
        }
        if !expanded {
            if pending.contains_key(&id) {
                continue;
            }
            let commit = objects
                .commit(&id)
                .map_err(|e| NotYet::unreadable(format!("commit {}", id.to_hex()), e))?;
            let own = u64::try_from(commit.committer.time)
                .unwrap_or(0)
                .min(ceiling);
            stack.push((id, true));
            for parent in &commit.parents {
                if !nodes.contains_key(parent) {
                    stack.push((*parent, false));
                }
            }
            let known = devices.len();
            let device = *devices.entry(commit.committer.email).or_insert(known);
            pending.insert(id, (commit.tree, commit.parents, own, device));
            continue;
        }
        let Some((tree, parents, own, device)) = pending.remove(&id) else {
            continue;
        };
        let mut eff = own;
        for parent in &parents {
            // A parent is finished before its child; one that is not is a loop, which a hash
            // cannot make.
            let Some(done) = nodes.get(parent) else {
                return Err(NotYet::unreadable(
                    format!("commit {}", id.to_hex()),
                    "its history loops back on itself",
                ));
            };
            eff = eff.max(done.eff);
        }
        nodes.insert(
            id,
            Node {
                tree,
                parents,
                eff,
                device,
            },
        );
        order.push(id);
    }
    Ok((nodes, order))
}

/// Reads the data objects the trees name, each tree and each pointer once however many commits
/// share them.
struct Walk<'a> {
    objects: &'a Objects,
    seen_trees: HashSet<(GitId, bool)>,
    seen_blobs: HashSet<GitId>,
}

impl<'a> Walk<'a> {
    fn new(objects: &'a Objects) -> Walk<'a> {
        Walk {
            objects,
            seen_trees: HashSet::new(),
            seen_blobs: HashSet::new(),
        }
    }

    /// Adds to `into` the data objects named by the pointer files below the root tree `root` that
    /// no earlier walk of this value has read. The drive's own `.azlin` folder holds no files of
    /// the drive (the policy, the members' keys) but its `sync` bookkeeping, which is sealed like
    /// every file.
    fn refs(&mut self, root: &GitId, into: &mut HashMap<ObjectId, u64>) -> Result<(), NotYet> {
        self.walk_from(vec![(*root, true)], into)
    }

    /// [`Walk::refs`] below a folder that is no root.
    fn subtree_refs(
        &mut self,
        tree: &GitId,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        self.walk_from(vec![(*tree, false)], into)
    }

    fn walk_from(
        &mut self,
        mut stack: Vec<(GitId, bool)>,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        while let Some((tree, is_root)) = stack.pop() {
            if !self.seen_trees.insert((tree, is_root)) {
                continue;
            }
            let folder = shard::read_folder(self.objects, &tree)
                .map_err(|e| NotYet::unreadable(format!("folder {}", tree.to_hex()), e))?;
            for entry in folder.entries() {
                match entry.mode {
                    Mode::Tree
                        if is_root && crate::names::Protected::is_bookkeeping(&entry.name) =>
                    {
                        self.bookkeeping(&entry.id, &mut stack)?;
                    }
                    Mode::Tree => stack.push((entry.id, false)),
                    Mode::File => self.pointer(&entry.id, &entry.name, into)?,
                }
            }
        }
        Ok(())
    }

    /// Adds to `into` the data objects the root tree `from` names that the root tree `to` does not
    /// name at the same place: what the change from one to the other dropped or replaced. Only
    /// the folders that differ are read, so a commit that changes one file costs one path.
    fn dropped(
        &mut self,
        from: &GitId,
        to: &GitId,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        let mut stack: Vec<(GitId, GitId, bool)> = vec![(*from, *to, true)];
        while let Some((before, after, is_root)) = stack.pop() {
            if before == after {
                continue;
            }
            let was = shard::read_folder(self.objects, &before)
                .map_err(|e| NotYet::unreadable(format!("folder {}", before.to_hex()), e))?;
            let now = shard::read_folder(self.objects, &after)
                .map_err(|e| NotYet::unreadable(format!("folder {}", after.to_hex()), e))?;
            let now_by_name: HashMap<&str, &TreeEntry> = now
                .entries()
                .iter()
                .map(|entry| (entry.name.as_str(), entry))
                .collect();
            for entry in was.entries() {
                let same_place = now_by_name.get(entry.name.as_str()).copied();
                let bookkeeping = is_root && crate::names::Protected::is_bookkeeping(&entry.name);
                match (entry.mode, same_place) {
                    (mode, Some(other)) if other.mode == mode && other.id == entry.id => {}
                    (Mode::Tree, Some(other)) if other.mode == Mode::Tree && bookkeeping => {
                        self.sync_pair(&entry.id, &other.id, &mut stack, into)?;
                    }
                    (Mode::Tree, Some(other)) if other.mode == Mode::Tree => {
                        stack.push((entry.id, other.id, false));
                    }
                    (Mode::Tree, _) if bookkeeping => {
                        let mut below = Vec::new();
                        self.bookkeeping(&entry.id, &mut below)?;
                        self.walk_from(below, into)?;
                    }
                    (Mode::Tree, _) => self.subtree_refs(&entry.id, into)?,
                    (Mode::File, _) => self.pointer(&entry.id, &entry.name, into)?,
                }
            }
        }
        Ok(())
    }

    /// The `sync` folders of two versions of `.azlin`: compared like any folders.
    fn sync_pair(
        &mut self,
        before: &GitId,
        after: &GitId,
        stack: &mut Vec<(GitId, GitId, bool)>,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        let find = |walk: &Walk<'_>, azlin: &GitId| -> Result<Option<GitId>, NotYet> {
            let folder = shard::read_folder(walk.objects, azlin)
                .map_err(|e| NotYet::unreadable(crate::names::BOOKKEEPING, e))?;
            Ok(folder
                .entries()
                .iter()
                .find(|e| e.mode == Mode::Tree && e.name == "sync")
                .map(|e| e.id))
        };
        match (find(self, before)?, find(self, after)?) {
            (Some(was), Some(now)) => stack.push((was, now, false)),
            (Some(was), None) => self.subtree_refs(&was, into)?,
            (None, _) => {}
        }
        Ok(())
    }

    /// The `sync` folder of the drive's `.azlin` folder, if it has one.
    fn bookkeeping(&self, azlin: &GitId, stack: &mut Vec<(GitId, bool)>) -> Result<(), NotYet> {
        let folder = shard::read_folder(self.objects, azlin)
            .map_err(|e| NotYet::unreadable(crate::names::BOOKKEEPING, e))?;
        if let Some(sync) = folder
            .entries()
            .iter()
            .find(|e| e.mode == Mode::Tree && e.name == "sync")
        {
            stack.push((sync.id, false));
        }
        Ok(())
    }

    /// The data object a pointer file names. A pointer that does not read - a newer format, a
    /// damaged one - is not "names nothing": it stops the decision.
    fn pointer(
        &mut self,
        blob: &GitId,
        name: &str,
        into: &mut HashMap<ObjectId, u64>,
    ) -> Result<(), NotYet> {
        if !self.seen_blobs.insert(*blob) {
            return Ok(());
        }
        let bytes = self
            .objects
            .blob(blob)
            .map_err(|e| NotYet::unreadable(name, e))?;
        // Strict: a line this reader does not know may name an object it cannot see, and an object
        // nothing is seen to name is deleted (X-DELTA-AZDRIVE-SYNC-6).
        let entry =
            pointer::decode_strict(bytes).map_err(|reason| NotYet::unreadable(name, reason))?;
        if let Some(object) = entry.object {
            let stored = object
                .packed
                .as_ref()
                .map_or(object.stored_size, |packed| packed.pack.object_len);
            into.entry(object.bucket_object()).or_insert(stored);
        }
        Ok(())
    }
}

/// Which versions of a drive an analysis keeps.
#[derive(Clone, Copy)]
enum Keep<'a> {
    /// The reclaimer's: the versions of the last `retention.versions`, and the state that span began
    /// with.
    Span(&'a Retention),
    /// A purge the user confirmed at this time (seconds since 1970; [`purge_options`]): what the head
    /// names and what a commit made after it FIRST names. Every other object the history released
    /// goes, whatever its age and whichever commit released it.
    After(u64),
}

/// What the history of `head` says of the drive's data objects at `now` (the time of day by the
/// caller's clock): the objects the head and the window name, and - unless `live_only` - the
/// objects the old commits (after `after`, at most `caps.commits` of them) named and nothing live
/// does. Pure: it reads `objects` and nothing else.
pub(crate) fn analyse(
    objects: &Objects,
    head: &GitId,
    now: u64,
    retention: &Retention,
    after: Option<&Mark>,
    caps: &Caps,
    live_only: bool,
) -> Result<Analysis, NotYet> {
    analyse_by(
        objects,
        head,
        now,
        Keep::Span(retention),
        after,
        caps,
        live_only,
    )
}

/// [`analyse`] for a purge the user confirmed at `confirmed_at` ([`purge_options`]): the window is
/// the commits made after it, and what is kept is what the head names and what those commits first
/// name - not the state the confirmation found, which is what the purge is for. Every commit's
/// releases are scanned, the ones made after the confirmation too (a re-encryption pass replaces the
/// old objects after the click).
pub(crate) fn analyse_confirmed(
    objects: &Objects,
    head: &GitId,
    now: u64,
    confirmed_at: u64,
    caps: &Caps,
) -> Result<Analysis, NotYet> {
    analyse_by(
        objects,
        head,
        now,
        Keep::After(confirmed_at),
        None,
        caps,
        false,
    )
}

fn analyse_by(
    objects: &Objects,
    head: &GitId,
    now: u64,
    keep: Keep<'_>,
    after: Option<&Mark>,
    caps: &Caps,
    live_only: bool,
) -> Result<Analysis, NotYet> {
    let (nodes, order) = load_graph(objects, head, now)?;
    let window_start = match keep {
        // A retention of nothing keeps no state but the head: no commit is inside the window.
        Keep::Span(retention) if retention.versions.is_zero() => u64::MAX,
        Keep::Span(retention) => now.saturating_sub(retention.versions.as_secs()),
        // Absolute, from the confirmation's own record: a span measured from a clock read before the
        // repository was opened starts late by the time the open takes (X-DELTA-SYNC6-1).
        Keep::After(confirmed_at) => confirmed_at.saturating_add(1),
    };
    let in_window = |id: &GitId| nodes.get(id).is_some_and(|node| node.eff >= window_start);
    let confirmed = matches!(keep, Keep::After(_));

    // The window's commits, and the old commits it grew out of: the state the window began with. A
    // purge scans the releases of every commit and keeps no state a commit before its confirmation
    // names.
    let mut window: Vec<GitId> = Vec::new();
    let mut frontier: HashSet<GitId> = HashSet::new();
    let mut old: Vec<GitId> = Vec::new();
    for id in &order {
        if in_window(id) {
            window.push(*id);
            if confirmed {
                old.push(*id);
            } else if let Some(node) = nodes.get(id) {
                frontier.extend(node.parents.iter().filter(|parent| !in_window(parent)));
            }
        } else {
            old.push(*id);
        }
    }

    let mut walk = Walk::new(objects);
    let mut live: HashMap<ObjectId, u64> = HashMap::new();
    let head_node = nodes
        .get(head)
        .ok_or_else(|| NotYet::unreadable("the head", "it is not in the history"))?;
    walk.refs(&head_node.tree, &mut live)?;
    let head_names: HashSet<ObjectId> = live.keys().copied().collect();
    if confirmed {
        first_named_after(objects, &nodes, &order, &in_window, &mut live)?;
    } else {
        for id in window.iter().chain(frontier.iter()) {
            if let Some(node) = nodes.get(id) {
                walk.refs(&node.tree, &mut live)?;
            }
        }
    }

    let mut analysis = Analysis {
        revision: 0,
        now,
        started: crate::time::Stopwatch::start(),
        window_start,
        commits: order.len(),
        window_commits: window.len(),
        live,
        head: head_names,
        candidates: BTreeMap::new(),
        scanned: 0,
        cursor: after.copied(),
        stopped_by_cap: false,
        devices: window
            .iter()
            .filter_map(|id| nodes.get(id).map(|node| node.device))
            .collect::<HashSet<usize>>()
            .len(),
        window_agrees: None,
    };
    if live_only {
        return Ok(analysis);
    }

    // The old commits, oldest first, from where the last run stopped.
    let mut todo: Vec<Mark> = old
        .iter()
        .filter_map(|id| {
            nodes.get(id).map(|node| Mark {
                time: node.eff,
                id: *id,
            })
        })
        .filter(|mark| after.is_none_or(|done| mark > done))
        .collect();
    todo.sort();
    let mut candidate_bytes = 0u64;
    for mark in todo {
        if analysis.scanned >= caps.commits
            || analysis.candidates.len() >= caps.objects
            || candidate_bytes >= caps.candidate_bytes
        {
            analysis.stopped_by_cap = true;
            break;
        }
        let Some(node) = nodes.get(&mark.id) else {
            continue;
        };
        // What this commit released: what its parents named and it does not.
        let mut named: HashMap<ObjectId, u64> = HashMap::new();
        for parent in &node.parents {
            if let Some(parent) = nodes.get(parent) {
                walk.dropped(&parent.tree, &node.tree, &mut named)?;
            }
        }
        for (id, bytes) in named {
            if analysis.live.contains_key(&id) {
                continue;
            }
            if let std::collections::btree_map::Entry::Vacant(slot) = analysis.candidates.entry(id)
            {
                candidate_bytes = candidate_bytes.saturating_add(bytes);
                slot.insert(Candidate {
                    bytes,
                    by: mark.id,
                    at: mark.time,
                });
            }
        }
        analysis.scanned += 1;
        analysis.cursor = Some(mark);
    }
    Ok(analysis)
}

/// Adds to `live` the objects FIRST named by a commit inside the window: named by the tree of some
/// commit there and by the tree of no commit before it. What a purge keeps beside the head
/// ([`Keep::After`]): a version made after the user's confirmation. An object a commit before the
/// confirmation named is not kept for being named by a later one too - a file the re-encryption pass
/// has not reached yet is named by every commit until it does, and the pass replaces it afterwards.
fn first_named_after(
    objects: &Objects,
    nodes: &HashMap<GitId, Node>,
    order: &[GitId],
    in_window: &dyn Fn(&GitId) -> bool,
    live: &mut HashMap<ObjectId, u64>,
) -> Result<(), NotYet> {
    // One walk and one map for each side: a walk reads a tree and a pointer once for the map it
    // fills, so the two sides must not share one.
    let mut before: HashMap<ObjectId, u64> = HashMap::new();
    let mut after: HashMap<ObjectId, u64> = HashMap::new();
    let mut walk_before = Walk::new(objects);
    let mut walk_after = Walk::new(objects);
    for id in order {
        let Some(node) = nodes.get(id) else {
            continue;
        };
        if in_window(id) {
            walk_after.refs(&node.tree, &mut after)?;
        } else {
            walk_before.refs(&node.tree, &mut before)?;
        }
    }
    for (id, bytes) in after {
        if !before.contains_key(&id) {
            live.entry(id).or_insert(bytes);
        }
    }
    Ok(())
}

/// What a restore of the drive could still bring back at `now`, derived the way the restore dialog
/// derives it (azcloud-kit `restore::file_versions`) and NOT the way [`analyse`] derives its window
/// (X-DELTA-SYNC2-2): along the drive's own history - the first parent of each commit, newest first -
/// and by the time each commit says it was made at, not the effective time of the whole graph. A
/// version is offered while the commit that REPLACED it is inside [`RETENTION`]`.versions` (the
/// dialog's own constant, never the options of the run), and the head's objects always are. The
/// answer is every object so offered, with its bytes.
///
/// The `live` set of a correct analysis holds all of it: every such commit is inside the analysis's
/// window by its effective time (which is never earlier than its own time), and so is the state it
/// replaced or the state the window began with. One that does not is a window that is not the one the
/// user was promised - a retention that drifted, a frontier that lost a state - and a delete from it
/// would take a version the dialog still offers. What the two share is the reading of a tree
/// ([`Walk`]); the choice of the commits, the clock they are judged by and the walk of the graph are
/// each one's own.
///
/// # Errors
///
/// [`NotYet::Unreadable`] when a commit or a tree cannot be read: nothing is decided from it.
pub(crate) fn restorable(
    objects: &Objects,
    head: &GitId,
    now: u64,
) -> Result<HashMap<ObjectId, u64>, NotYet> {
    let window_start =
        i64::try_from(now.saturating_sub(RETENTION.versions.as_secs())).unwrap_or(i64::MAX);
    let mut walk = Walk::new(objects);
    let mut offered: HashMap<ObjectId, u64> = HashMap::new();
    let mut commit = objects
        .commit(head)
        .map_err(|e| NotYet::unreadable("the head", e))?;
    // What the head names is the current version of every file: always offered.
    walk.refs(&commit.tree, &mut offered)?;
    while let Some(parent) = commit.parents.first().copied() {
        let before = objects
            .commit(&parent)
            .map_err(|e| NotYet::unreadable(format!("commit {}", parent.to_hex()), e))?;
        // What this commit replaced or deleted was a version of a file until it did: offered while
        // the commit is inside the window.
        if commit.committer.time >= window_start {
            walk.dropped(&before.tree, &commit.tree, &mut offered)?;
        }
        commit = before;
    }
    Ok(offered)
}

impl Analysis {
    /// The proof that `id` may leave the bucket, if the analysis shows it released by an old
    /// commit and named by nothing live.
    pub(crate) fn prove(&self, id: &ObjectId) -> Result<Reclaimable, NotYet> {
        if self.live.contains_key(id) {
            return Err(NotYet::Named);
        }
        if !self.candidates.contains_key(id) {
            return Err(NotYet::NotReleased);
        }
        Ok(Reclaimable {
            id: *id,
            _sealed: (),
        })
    }
}

/// The data objects the head and every state inside the retention window name, read fresh and
/// whole from the drive (an index in memory has none: its head is all there is).
#[derive(Debug)]
pub struct LiveObjects {
    live: HashSet<ObjectId>,
    /// The ones the head alone names (all of `live` for an index that keeps no history).
    head: HashSet<ObjectId>,
    now: u64,
}

impl LiveObjects {
    /// The objects `named` - a head of an index that keeps no history, which is everything there
    /// is to be named.
    pub(crate) fn of(named: HashSet<ObjectId>, now: u64) -> LiveObjects {
        LiveObjects {
            head: named.clone(),
            live: named,
            now,
        }
    }

    /// A drive with no commit yet names nothing.
    pub(crate) fn empty(now: u64) -> LiveObjects {
        LiveObjects {
            live: HashSet::new(),
            head: HashSet::new(),
            now,
        }
    }

    pub(crate) fn from_analysis(analysis: &Analysis) -> LiveObjects {
        LiveObjects {
            live: analysis.live.keys().copied().collect(),
            head: analysis.head.clone(),
            now: analysis.now,
        }
    }

    /// The time of day the window was judged by.
    #[must_use]
    pub fn now(&self) -> u64 {
        self.now
    }

    /// Whether the head or the window names `id`.
    #[must_use]
    pub fn names(&self, id: &ObjectId) -> bool {
        self.live.contains(id)
    }

    /// Whether the head itself names `id`: a version a restore brought back. What only the window
    /// names is held by the history, and is still released as far as the head goes.
    #[must_use]
    pub fn names_in_head(&self, id: &ObjectId) -> bool {
        self.head.contains(id)
    }

    /// The proof that `id` - an object the caller knows was uploaded and may be named by nothing
    /// (an upload whose commit never came, a pack its files all left) - may leave the bucket:
    /// the head and the window do not name it.
    ///
    /// # Errors
    ///
    /// [`NotYet::Named`] when they do.
    pub fn prove_unnamed(&self, id: &ObjectId) -> Result<Reclaimable, NotYet> {
        if self.live.contains(id) {
            return Err(NotYet::Named);
        }
        Ok(Reclaimable {
            id: *id,
            _sealed: (),
        })
    }
}

/// Of the pointer files `blobs` (ids in `repo`'s copy, which must hold them), the ones whose data
/// object is no longer in the bucket: a restore that commits such a pointer names an object that
/// is gone, and the file it replaces - a good one - would read "its object is missing" from then
/// on (AZ-U24: the retention had removed the object). A blob that is no pointer, or one of a
/// newer format, names nothing this can judge and is not reported; a bucket that cannot be asked
/// is an error, never "it is there". Each object is asked once.
///
/// # Errors
///
/// The repository's (a blob the copy does not hold) or the bucket's.
pub fn gone_objects<B: Bucket, S: Sealer>(
    repo: &MetaRepo<B, S>,
    blobs: &[GitId],
) -> Result<Vec<GitId>, MetaError> {
    let mut asked: HashMap<ObjectId, bool> = HashMap::new();
    let mut gone = Vec::new();
    for blob in blobs {
        let bytes = repo.objects().blob(blob)?;
        let Ok(entry) = pointer::decode(bytes) else {
            continue;
        };
        let Some(object) = entry.bucket_object() else {
            continue;
        };
        let there = match asked.get(&object) {
            Some(there) => *there,
            None => {
                let there = repo.store().bucket().exists(&object.bucket_key())?;
                asked.insert(object, there);
                there
            }
        };
        if !there {
            gone.push(*blob);
        }
    }
    Ok(gone)
}

/// What a reclaimer run found and did.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReclaimReport {
    pub mode: ReclaimMode,
    /// The commits before this time (seconds since 1970) were older than the retention.
    pub window_start: u64,
    /// The commits reachable from the head, and those inside the window.
    pub commits: usize,
    pub window_commits: usize,
    /// The old commits whose trees were read.
    pub scanned: usize,
    /// The data objects the head and the window name.
    pub live_objects: usize,
    /// The objects the history shows released past the retention, and their bytes in the bucket:
    /// what a run in [`ReclaimMode::Delete`] deletes - in shadow mode, "would delete N objects,
    /// M bytes".
    pub candidates: usize,
    pub candidate_bytes: u64,
    pub deleted: usize,
    pub deleted_bytes: u64,
    /// Deletes the bucket refused (tried again at the next run).
    pub failed: usize,
    /// A cap ended the scan before the old commits did.
    pub capped: bool,
    /// Why the run did not go through, if it did not.
    pub stopped: Option<NotYet>,
    /// SHA-256 (hex) over the candidates' ids in order: two runs over one history that say the
    /// same say the same digest, and a run whose candidates changed says another (empty: no
    /// analysis).
    pub digest: String,
    /// The effective times of the oldest and of the newest commit that released a candidate.
    pub releasing: Option<(u64, u64)>,
    /// The bytes of the packs the run read (what the manifest said before it fetched them).
    pub pack_bytes: u64,
    /// What an independent listing of the head said of the live set ([`GoEvidence::rederived_live_equal`]).
    pub rederived: Option<bool>,
    /// What the restore dialog's own derivation of the window said of the live set
    /// ([`GoEvidence::window_rederived`]).
    pub window_rederived: Option<bool>,
    /// The devices that committed inside the window.
    pub devices: u32,
    /// What the shadow rounds recorded, when the run asked ([`go`]).
    pub evidence: Option<GoEvidence>,
    /// Why a run configured to delete only reported.
    pub held: Option<Hold>,
}

impl ReclaimReport {
    /// The report of a round that had nothing to read (a drive without an index yet).
    #[must_use]
    pub fn none(mode: ReclaimMode) -> ReclaimReport {
        ReclaimReport::new(mode)
    }

    fn new(mode: ReclaimMode) -> ReclaimReport {
        ReclaimReport {
            mode,
            window_start: 0,
            commits: 0,
            window_commits: 0,
            scanned: 0,
            live_objects: 0,
            candidates: 0,
            candidate_bytes: 0,
            deleted: 0,
            deleted_bytes: 0,
            failed: 0,
            capped: false,
            stopped: None,
            digest: String::new(),
            releasing: None,
            pack_bytes: 0,
            rederived: None,
            window_rederived: None,
            devices: 0,
            evidence: None,
            held: None,
        }
    }

    /// The report of a run that did not read the drive because its index is over the budget.
    #[must_use]
    pub fn too_big(mode: ReclaimMode, bytes: u64, budget: u64) -> ReclaimReport {
        let mut report = ReclaimReport::new(mode);
        report.pack_bytes = bytes;
        report.stopped = Some(NotYet::TooBig { bytes, budget });
        report
    }

    fn take(&mut self, analysis: &Analysis) {
        self.window_start = analysis.window_start;
        self.commits = analysis.commits;
        self.window_commits = analysis.window_commits;
        self.scanned = analysis.scanned;
        self.live_objects = analysis.live.len();
        self.candidates = analysis.candidates.len();
        self.candidate_bytes = analysis.candidates.values().map(|c| c.bytes).sum();
        self.capped = analysis.stopped_by_cap;
        self.digest = digest_of(analysis.candidates.keys());
        self.releasing = analysis
            .candidates
            .values()
            .map(|c| c.at)
            .fold(None, |range, at| match range {
                None => Some((at, at)),
                Some((lo, hi)) => Some((lo.min(at), hi.max(at))),
            });
        self.devices = u32::try_from(analysis.devices).unwrap_or(u32::MAX);
        self.window_rederived = analysis.window_agrees;
    }

    /// The report as one line: what the log says.
    #[must_use]
    pub fn line(&self) -> String {
        let how = match self.mode {
            ReclaimMode::Off => return String::from("reclaim: switched off"),
            ReclaimMode::Shadow => "reclaim (shadow): would delete",
            ReclaimMode::Delete => "reclaim: deleted",
        };
        let (count, bytes) = if self.mode == ReclaimMode::Delete {
            (self.deleted, self.deleted_bytes)
        } else {
            (self.candidates, self.candidate_bytes)
        };
        let mut line = format!(
            "{how} {count} objects, {bytes} bytes ({} commits, {} in the window, {} read, {} \
             objects live)",
            self.commits, self.window_commits, self.scanned, self.live_objects
        );
        if self.failed > 0 {
            line.push_str(&format!(", {} not deleted", self.failed));
        }
        if self.capped {
            line.push_str(", stopped at a cap");
        }
        if !self.digest.is_empty() {
            line.push_str(&format!(
                ", digest {}",
                &self.digest[..self.digest.len().min(16)]
            ));
        }
        if let Some((oldest, newest)) = self.releasing {
            line.push_str(&format!(", released by commits of {oldest}..{newest}"));
        }
        match self.rederived {
            Some(true) => line.push_str(", the head listed again: agrees"),
            Some(false) => line.push_str(", the head listed again: DISAGREES"),
            None => {}
        }
        match self.window_rederived {
            Some(true) => line.push_str(", the restore's own window: agrees"),
            Some(false) => line.push_str(", the restore's own window: DISAGREES"),
            None => {}
        }
        if let Some(e) = &self.evidence {
            line.push_str(&format!(
                ", clean rounds {} over {} days by {} of {} devices",
                e.clean_rounds, e.days, e.devices, e.drive_devices
            ));
        }
        if let Some(why) = &self.held {
            line.push_str(&format!(", held back: {why}"));
        }
        if let Some(why) = &self.stopped {
            line.push_str(&format!(", stopped: {why}"));
        }
        line
    }
}

/// SHA-256 (hex) of the ids `ids` in the order given: the BTreeMap's, so the same candidates say the
/// same digest on every device.
fn digest_of<'a>(ids: impl Iterator<Item = &'a ObjectId>) -> String {
    let mut hasher = Sha256::new();
    for id in ids {
        hasher.update(id.bucket_key().as_bytes());
        hasher.update([0]);
    }
    hex(&hasher.finalize())
}

/// One run of the reclaimer over `repo`'s drive. The copy is read fresh and whole first. Without a
/// licence in `gate` (the answer of [`ask_retired_delete`]) that is all: the report says what would
/// go. With one, in a run configured as [`ReclaimMode::Delete`], the run takes the maintenance
/// lease, checks the bucket's clock against this device's, and for each batch of proven objects (1)
/// fences - the manifest is swapped, and only while it is still the revision the reads were made at,
/// so nobody published since -, (2) deletes, and when every delete went through (3) records how far
/// it got. A delete the bucket refuses stops the mark: the next run finds the object again and tries
/// again. Nothing is deleted from reads that are old.
///
/// # Errors
///
/// The repository's or the bucket's, for a run that could not even read the drive; a run that
/// stops on a decision (a clock, a lease, a drive that moved) is a report with `stopped`.
pub fn reclaim<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    options: &ReclaimOptions,
    gate: Gate,
) -> Result<ReclaimReport, MetaError> {
    let mut asked = Some(gate);
    reclaim_judged(repo, options, &mut |_, _| {
        asked.take().unwrap_or(Err(Hold::NotJudged))
    })
}

/// What a run asks after its analysis: may it delete? It sees the analysis (and may write what it
/// found into the report) and answers with the gate - the licence only from [`ask_retired_delete`]
/// (or a purge the user confirmed), else what holds the delete back.
pub(crate) type Judge<'a> = dyn FnMut(&Analysis, &mut ReclaimReport) -> Gate + 'a;

/// [`reclaim`], with `judge` asking after the analysis: how the automatic round of a provider asks
/// the evidence of the drive's shadow rounds before it deletes anything ([`ask_retired_delete`]).
pub(crate) fn reclaim_judged<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    options: &ReclaimOptions,
    judge: &mut Judge<'_>,
) -> Result<ReclaimReport, MetaError> {
    let mut report = ReclaimReport::new(options.mode);
    if options.mode == ReclaimMode::Off {
        report.stopped = Some(NotYet::Off);
        return Ok(report);
    }
    repo.pull()?;
    repo.fetch_all()?;
    let Some(head) = repo.head() else {
        return Ok(report);
    };
    let now = repo.store().now();
    let started = crate::time::Stopwatch::start();
    let revision = repo.store().state().revision;
    let after = repo
        .store()
        .manifest()
        .and_then(|manifest| manifest.reclaimed.as_ref())
        .and_then(Mark::from_wal)
        .filter(|_| options.use_mark);
    // A confirmation dated after this device's clock covers what cannot be told: nothing is erased.
    if let Some(at) = options
        .purge_after
        .filter(|at| *at > now.saturating_add(SKEW_SECS))
    {
        report.stopped = Some(NotYet::Confirmation { at, now });
        return Ok(report);
    }
    let analysed = match options.purge_after {
        Some(at) => analyse_confirmed(repo.objects(), &head, now, at, &options.caps),
        None => analyse(
            repo.objects(),
            &head,
            now,
            &options.retention,
            after.as_ref(),
            &options.caps,
            false,
        ),
    };
    let mut analysis = match analysed {
        Ok(analysis) => analysis,
        Err(why) => {
            report.stopped = Some(why);
            return Ok(report);
        }
    };
    analysis.revision = revision;
    analysis.started = started;
    // What the restore dialog still offers, derived its own way, against the window just analysed.
    // collapse-ok: a derivation that cannot be made is "not checked" (`None`), and a round that was
    // not checked is not clean: the failure only holds the switch back.
    analysis.window_agrees = restorable(repo.objects(), &head, now)
        .ok()
        .map(|offered| offered.keys().all(|id| analysis.live.contains_key(id)));
    report.take(&analysis);
    report.pack_bytes = pack_bytes_of(repo);
    let gate = judge(&analysis, &mut report);
    // A licence deletes in a run configured to delete, and in none other.
    let deletes = options.mode == ReclaimMode::Delete && gate.is_ok();
    report.mode = match &gate {
        _ if deletes => ReclaimMode::Delete,
        Err(Hold::Off) => ReclaimMode::Off,
        _ => ReclaimMode::Shadow,
    };
    if !deletes {
        return Ok(report);
    }
    let mut guard = match repo.store_mut().acquire_lease(MAINTENANCE, LEASE_SECS) {
        Ok(guard) => guard,
        Err(MetaError::LeaseHeld { .. }) => {
            report.stopped = Some(NotYet::Lease);
            return Ok(report);
        }
        Err(e) => return Err(e),
    };
    let done = delete_under(
        repo,
        &mut guard,
        &analysis,
        after.as_ref(),
        options,
        &mut report,
        &gate,
    );
    let released = repo.store_mut().release_lease(guard);
    done?;
    released?;
    Ok(report)
}

/// The bytes of the live packs the manifest of `repo` names (packs and their indexes).
#[must_use]
pub(crate) fn pack_bytes_of<B: Bucket, S: Sealer>(repo: &MetaRepo<B, S>) -> u64 {
    repo.store()
        .manifest()
        .map_or(0, |manifest| manifest_pack_bytes(&manifest.packs))
}

/// The bytes of `packs` (the pack and the index of each).
#[must_use]
pub(crate) fn manifest_pack_bytes(packs: &[super::wal::PackRef]) -> u64 {
    packs.iter().fold(0u64, |total, pack| {
        total
            .saturating_add(pack.pack_size)
            .saturating_add(pack.idx_size)
    })
}

/// The deleting half of [`reclaim`], under the lease.
fn delete_under<B: Bucket, S: Sealer>(
    repo: &mut MetaRepo<B, S>,
    guard: &mut LeaseGuard,
    analysis: &Analysis,
    after: Option<&Mark>,
    options: &ReclaimOptions,
    report: &mut ReclaimReport,
    gate: &Gate,
) -> Result<(), MetaError> {
    // The analysis took the time it took (a big drive's takes minutes), and the lease this run wrote
    // just now is compared with the time of day the analysis was made at, carried forward by the
    // monotonic clock - not with the device's clock of this moment, which a step during the analysis
    // makes agree with the bucket while the window was cut by the other (X-DELTA-SYNC4-4).
    let now = analysis.now.saturating_add(analysis.started.secs());
    if let Err(why) = check_clock(repo, guard, now, options.server_clock) {
        report.stopped = Some(why);
        return Ok(());
    }
    let ids: Vec<ObjectId> = analysis.candidates.keys().copied().collect();
    let mut revision = analysis.revision;
    for batch in ids.chunks(options.caps.batch.max(1)) {
        // The kill switch is looked at before every batch: a hold set while the run works stops it at the
        // next one, and a switch that cannot be read stops it too (X-DELTA-AZDRIVE-SYNC-6).
        match Switch::read_bucket(repo.store().bucket()) {
            Switch::Clear => {}
            other => {
                report.stopped = Some(NotYet::Switch(other));
                return Ok(());
            }
        }
        repo.store_mut().renew_lease(guard, LEASE_SECS)?;
        // The fence: the manifest is still the revision the reads were made at.
        revision = match repo.store_mut().stamp(guard, revision, None) {
            Ok(revision) => revision,
            Err(MetaError::Conflict { .. } | MetaError::OutcomeUnknown { .. }) => {
                report.stopped = Some(NotYet::Moved);
                return Ok(());
            }
            Err(e) => return Err(e),
        };
        for id in batch {
            let proof = match analysis.prove(id) {
                Ok(proof) => proof,
                Err(why) => {
                    report.stopped = Some(why);
                    return Ok(());
                }
            };
            match remove_reclaimable(repo.store().bucket(), gate, proof) {
                Ok(_) => {
                    report.deleted += 1;
                    report.deleted_bytes += analysis.candidates.get(id).map_or(0, |c| c.bytes);
                }
                Err(_) => report.failed += 1,
            }
        }
    }
    // Every delete went through: the mark moves, so the next run starts after these commits.
    if report.failed == 0 && options.use_mark {
        if let Some(cursor) = analysis.cursor.filter(|cursor| after != Some(cursor)) {
            let mark = cursor.to_wal(repo.store().now());
            match repo.store_mut().stamp(guard, revision, Some(mark)) {
                Ok(_) => {}
                Err(MetaError::Conflict { .. } | MetaError::OutcomeUnknown { .. }) => {
                    report.stopped = Some(NotYet::Moved);
                }
                Err(e) => return Err(e),
            }
        }
    }
    Ok(())
}

/// The bucket's clock and this device's agree: the lease this run took was written just now, and
/// the bucket says when.
fn check_clock<B: Bucket, S: Sealer>(
    repo: &MetaRepo<B, S>,
    guard: &LeaseGuard,
    now: u64,
    policy: ServerClock,
) -> Result<(), NotYet> {
    if policy == ServerClock::Unchecked {
        return Ok(());
    }
    let key = super::keys::lease(&guard.purpose);
    let server = repo.store().bucket().modified_of(&key).ok().flatten();
    match server {
        Some(server) if stamp_agrees(server, now, 0) => Ok(()),
        _ => Err(NotYet::Clock {
            device: now,
            server,
        }),
    }
}
