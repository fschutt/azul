//! After a lost reply: did my `head_cas` land? (planning/architecture.md section 2.5 "A lost connection
//! or a timeout counts as elsewhere for verbs flagged idempotent; for the others (`head_cas`,
//! `log_append`, `team_log_append`) the client first reads whether the write landed", section 6.10 A7
//! "A lost race reads the head again, merges again and retries"; build unit S16, lane W1-S16a).
//!
//! A `head_cas` that expected version `E` can only ever have created version `E + 1` (version 1 when it
//! expected `None`): the register hands out versions one at a time, so exactly one write takes each
//! one. So the question has an exact answer, and only that one version needs looking at: if the head
//! has not reached it, the write did not land; if the register shows it and it is this device's own
//! write of this value, it landed (whoever advanced the head since); if the register shows it and it
//! is somebody else's write or a restore record, it did not; and if the register no longer shows it (a
//! page that does not hold it, or a history pruned past it) the answer is UNKNOWN, never "no": a
//! client that took "no" for "I cannot tell" would merge and send its change a second time.
//!
//! A server can lie to the client here; the repository's signed log entry, which the value points to,
//! is what makes a lie visible to the device, and it is the repository's to check. This function only
//! reads the answers it is given.

use azlin_seams::{mutant, DeviceId};

use crate::head::{HeadRecord, HeadValue, Writer};
use crate::history::HistoryPage;
use crate::version::Version;

/// What a device learns about a `head_cas` whose reply it lost.
///
/// Implements planning/architecture.md section 2.5 (the client first reads whether the write landed)
/// and section 6.10 A7.
///
/// Caller: `resolve_lost_cas`; SYNC-CLIENT (the commit path of the sync engine) decides by it.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
#[must_use]
pub enum LostCas {
    /// The write landed, as this version: do not send it again.
    Landed(Version),
    /// The write did not land: read the head, merge and try again.
    NotLanded,
    /// The answers given do not show it either way (the history is too short, or the page does not
    /// reach it): read more history, or the head again. Never treat this as `NotLanded`.
    Unknown,
}

/// Decides whether a `head_cas` landed, from the head and a page of history read after the reply was
/// lost. `expected` is the version the write expected (`None` for a create), `mine` the value it
/// wrote, `me` the writing device, `current` the head read now, `history` a page read from the
/// version the write would have created (`HeadRegister::history`).
///
/// Implements planning/architecture.md section 2.5 (the client first reads whether the write landed)
/// and section 6.10 A7.
///
/// Caller: SYNC-CLIENT (azcloud-kit's sync engine, after a lost connection or a timeout on
/// `head_cas`); S47c's restore stories.
pub fn resolve_lost_cas(
    expected: Option<Version>,
    mine: &HeadValue,
    me: DeviceId,
    current: &HeadRecord,
    history: &HistoryPage,
) -> LostCas {
    let want = match expected {
        None => Some(Version::FIRST),
        Some(version) => version.next(),
    };
    // A write that expected the largest version cannot have created a version above it.
    let Some(want) = want else {
        return LostCas::NotLanded;
    };
    // The head has not reached the version the write would have created: it did not land. (A head
    // below the expected version is a register that went back through an announced restore; the write
    // is lost with the rest of the lost timeline, and the device sends it again from the restore.)
    if current.version < want {
        return LostCas::NotLanded;
    }
    let found: Option<&HeadRecord> = if current.version == want {
        Some(current)
    } else {
        history.records.iter().find(|record| record.version == want)
    };
    match found {
        Some(record) => {
            // Mine means: a write (not a restore record) by this device, of this value.
            let by_me = record.is_write_by(me);
            let same_value = record.value == *mine;
            let landed = if mutant!("sync_lost_reply_ignores_writer") {
                same_value
            } else if mutant!("sync_lost_reply_counts_restores") {
                record.writer == Writer::Device(me) && same_value
            } else if mutant!("sync_lost_reply_ignores_value") {
                by_me
            } else {
                by_me && same_value
            };
            if landed {
                LostCas::Landed(want)
            } else {
                LostCas::NotLanded
            }
        }
        // The register does not show the version: pruned, or outside the page. Never "no".
        None if mutant!("sync_lost_reply_unknown_is_not_landed") => LostCas::NotLanded,
        None => LostCas::Unknown,
    }
}
