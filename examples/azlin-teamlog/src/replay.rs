//! `replay`: a device's own check of a run of entries (planning/architecture.md section 5.9 "A new
//! device verifies from the latest checkpoint whose entries it can check, not from the account's first
//! entry"; "every device keeps the part of the chain it verified"; section 12 E1 "devices run their own
//! check, and the test runs both"; build unit S17, lane W1-S17c2).
//!
//! A device does not trust the metadata group's answer: it runs `verify_signatures`, `admit` and `apply`
//! itself, entry by entry, over the leader's stamps that the log carries (`team_log_get` returns each
//! entry with its stamp), and keeps the state those entries make. The metadata group runs the same three
//! functions on the same entries in the same order, so the two end with the same `state_hash` and the
//! first entry either of them refuses is the same one, with the same reason.
//!
//! `start` is the state to begin from: `TeamState::empty` to verify from the first entry, or the state a
//! verified checkpoint names (a device fetches the registry rows of the checkpoint, builds the state, and
//! compares `state_hash` with the checkpoint's before it trusts it).

use azlin_seams::{SignatureVerifier, Stamp};

use crate::admit::{admit, Invalid, Refusal};
use crate::apply::apply;
use crate::proposal::{RawEntry, TrustedKeys};
use crate::state::TeamState;
use crate::verify::verify_signatures;

/// Applies `entries` to `start` in order, checking each as the metadata group does, at the leader's
/// stamp `stamps[i]` of the entry. The state after the last entry, or the position of the first entry
/// that is refused and why. `stamps` must have one stamp for each entry; otherwise nothing is applied
/// and the answer is a refusal at the first position that has an entry and no stamp (or a stamp and no
/// entry).
///
/// Implements planning/architecture.md section 5.9 (a device verifies the chain it keeps) and section
/// 12 E1 (the same pure check on every device).
///
/// Caller: the sync client's verification of a fetched run (CHAIN-GLUE), a new device that starts from
/// a checkpoint; lane S47a runs it beside `admit` + `apply` and demands the same answer.
pub fn replay(
    start: TeamState,
    entries: &[RawEntry],
    v: &dyn SignatureVerifier,
    trusted: &TrustedKeys,
    stamps: &[Stamp],
) -> Result<TeamState, (usize, Refusal)> {
    if stamps.len() != entries.len() {
        return Err((
            entries.len().min(stamps.len()),
            Refusal::Invalid(Invalid::Args("one leader stamp for each entry")),
        ));
    }
    let mut state = start;
    for (index, (entry, stamp)) in entries.iter().zip(stamps.iter()).enumerate() {
        let proposal = verify_signatures(&state, entry, v, trusted).map_err(|why| (index, why))?;
        let admitted = admit(&state, &proposal, *stamp).map_err(|why| (index, why))?;
        if !apply(&mut state, admitted).applied {
            return Err((
                index,
                Refusal::Invalid(Invalid::StateDamaged("an admitted entry did not apply")),
            ));
        }
    }
    Ok(state)
}
