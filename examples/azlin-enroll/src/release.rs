//! The release decision: may the group serve the wrap to the new device now? (planning/architecture.md
//! section 7.5 "Notice and hold", section 5.9 "The group enforces holds by withholding the wrap
//! until `hold_until`", section 12 E2; build unit S17, lane W1-S17e).
//!
//! "No key is released before its hold ends on any path; an existing device sees every request and
//! can veto it" (E2). A key goes out in TWO steps, and the second one cannot be skipped by the
//! types of this crate:
//!
//! 1. [`release()`] judges what the request itself holds, and says yes only when ALL of these hold,
//!    checked in this order:
//!    1. the request is not cancelled and not expired (a request that was released already answers
//!       `AlreadyReleased`: no second grant, the group serves from its own record of the device);
//!    2. the leader's stamp is agreed ([`StampCheck::Agreed`]): a disputed stamp (a clock that
//!       jumped, refused by the Roughtime check of S40) never releases, whatever it says;
//!    3. the hold has ended: `now >= hold_until`, an instant fixed when the request was opened;
//!    4. the path's evidence is complete (every kind of its row of `PATHS`).
//!
//!    The answer is a [`ReleaseGrant`], a WITNESS: its fields are private and it has no `Clone`,
//!    `Default` or `From`, so only `release` makes one. It names the request and the stamp of the
//!    decision and NOTHING ELSE: not the wrap, not the device. It is the decision that the request
//!    may be finished, not the permission to serve a wrap.
//! 2. [`mark_released`] takes the grant and the facts of the moment, judges the STANDING of
//!    everybody the request depends on again (a device that approved and was revoked since, admins
//!    that signed and were suspended since, an escrow holder who is no admin any more, do not
//!    finish a request), moves the request to `Released` (so a cancel that came first wins and no
//!    wrap goes out after it) and returns [`Served`], the second WITNESS and the one value that
//!    says which wrap goes to which device. A group that serves a wrap holds a `Served` for it.
//!
//! `release` takes the stamp as a parameter and reads no clock; it does not change the request,
//! so a stamp that goes back neither releases, nor extends the hold, nor cancels anything.
//!
//! ```compile_fail
//! // A grant cannot be copied: only `release` makes one.
//! fn takes_a_grant(grant: azlin_enroll::ReleaseGrant) {
//!     let _second = grant.clone();
//! }
//! ```
//!
//! ```compile_fail
//! // A grant cannot be made out of nothing.
//! let _grant = azlin_enroll::ReleaseGrant::default();
//! ```
//!
//! ```compile_fail
//! // The permission to serve a wrap cannot be copied: only `mark_released` makes one.
//! fn takes_a_served(served: azlin_enroll::Served) {
//!     let _second = served.clone();
//! }
//! ```
//!
//! ```compile_fail
//! // A grant says nothing of the wrap or the device: those are in `Served` alone.
//! fn takes_a_grant(grant: azlin_enroll::ReleaseGrant) {
//!     let _wrap = grant.wrap();
//! }
//! ```

use azlin_policy::{distinct_people, Signatory};
use azlin_seams::{DeviceId, MemberId, Stamp};

use crate::escrow_open::{shortfall_for, Shortfall};
use crate::evidence::{confirmer_stands, satisfied, Evidence, EvidenceKind};
use crate::notices::{NoticeKind, Notices};
use crate::path::{Path, WrapKind};
use crate::request::{OpenedBy, PersonFacts, Request, RequestId, RequestState};

/// Whether the leader's stamp was agreed by the time check (S40: the Roughtime check of the
/// leader's clock).
///
/// Implements planning/architecture.md section 12 E2 ("a clock that jumps (refused by the
/// Roughtime check)") and section 2.4 (time is the leader's stamp, checked).
///
/// Caller: `release`, `mark_released`, `expire`, `open`, `add_evidence`. Seam: S40 (builds it from
/// the checked leader stamp).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum StampCheck {
    /// The check agreed with the leader's stamp.
    Agreed,
    /// The check disputed the stamp (it jumped, or no agreement was reached): never releases.
    Disputed,
}

/// Why a release is withheld.
///
/// Implements planning/architecture.md section 7.5 (the wrap is withheld until `hold_until`) and
/// section 12 E2.
///
/// Caller: `release`. Seam: S9c/ENROLL-WIRE (the answer `retry_after` with `hold_until`), S47b.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// An admin or an active device of the person cancelled the request.
    Cancelled,
    /// The request expired: nobody released it in time.
    Expired,
    /// The stamp is disputed: a disputed stamp never releases.
    StampDisputed,
    /// The hold has not ended.
    HoldNotElapsed {
        /// When it ends.
        hold_until: Stamp,
    },
    /// A kind of evidence of the path is missing (or incomplete).
    EvidenceMissing(EvidenceKind),
}

/// The decision that a request may be finished now: a witness only `release` makes. It names the
/// request and the stamp of the decision and nothing else (not the wrap, not the device): it is no
/// permission to serve a wrap, `mark_released` turns it into one after it judged the standing of
/// everybody the request depends on (see [`Served`]).
///
/// Implements planning/architecture.md section 7.5 (the group serves the wrap after the hold) and
/// section 12 E2 (the decision is taken by the one function that knows the hold and the evidence).
///
/// Caller: `release` (the only constructor), `mark_released` (consumes it). Seam: S9c/ENROLL-WIRE
/// (hands it to `mark_released` with the facts of the moment).
#[derive(Debug)]
pub struct ReleaseGrant {
    request: RequestId,
    at: Stamp,
}

impl ReleaseGrant {
    /// The request this grant is for.
    pub fn request(&self) -> RequestId {
        self.request
    }

    /// The stamp at which the release was decided.
    pub fn at(&self) -> Stamp {
        self.at
    }
}

/// The permission to serve the wrap of a request: a witness only `mark_released` makes, after the
/// hold ended, the evidence was complete AND everybody the request depends on still stood in the
/// facts of the moment. It is the one value that says which wrap goes to which device (a
/// `ReleaseGrant` does not), so a group that serves a wrap with the types of this crate holds one.
/// Its fields are private and it has no `Clone`, `Default` or `From`.
///
/// Implements planning/architecture.md section 7.5 (the group serves the wrap after the hold,
/// "any admin or any of the person's devices can cancel": the veto that came first wins) and
/// section 5.3 (the release is the group serving the `reg_wrap` row).
///
/// Caller: `mark_released` (the only constructor). Seam: S9c/ENROLL-WIRE (serves the wrap named by
/// `wrap()` to `new_device()` and emits `notices()`).
#[derive(Debug)]
pub struct Served {
    request: RequestId,
    person: MemberId,
    new_device: DeviceId,
    path: Path,
    wrap: WrapKind,
    at: Stamp,
    notices: Notices,
}

impl Served {
    /// The request that was released.
    pub fn request(&self) -> RequestId {
        self.request
    }

    /// The person whose key it is.
    pub fn person(&self) -> MemberId {
        self.person
    }

    /// The device the wrap goes to.
    pub fn new_device(&self) -> DeviceId {
        self.new_device
    }

    /// The path of the request.
    pub fn path(&self) -> Path {
        self.path
    }

    /// The wrap the group serves.
    pub fn wrap(&self) -> WrapKind {
        self.wrap
    }

    /// The stamp at which the release was decided.
    pub fn at(&self) -> Stamp {
        self.at
    }

    /// The notices the group must emit: the key was released.
    pub fn notices(&self) -> &Notices {
        &self.notices
    }

    /// The notices, by value.
    pub fn into_notices(self) -> Notices {
        self.notices
    }
}

/// The answer of [`release`].
///
/// Implements planning/architecture.md section 7.5 and section 12 E2.
///
/// Caller: `release`. Seam: S9c/ENROLL-WIRE, S47b.
#[derive(Debug)]
pub enum Release {
    /// The request may be finished; the grant is consumed by `mark_released`, which judges the
    /// standing and gives the permission to serve the wrap.
    Granted(ReleaseGrant),
    /// The wrap is withheld, and why.
    Withheld(Reason),
    /// The key was released at this stamp already: no second grant is made. A new device that did
    /// not get the answer is served from the group's own record of the device (active), not from
    /// a second grant, and a device that was removed since is never served again.
    AlreadyReleased {
        /// The stamp of the release.
        at: Stamp,
    },
}

impl Release {
    /// Whether the request may be finished (a grant was made).
    pub fn is_granted(&self) -> bool {
        matches!(self, Release::Granted(_))
    }
}

/// May this request be finished now? See the module for the four conditions. It judges what the
/// request itself holds and takes no facts: the standing of the ones the request depends on is
/// judged by `mark_released`, which turns the grant into the permission to serve (`Served`). The
/// guards `enroll_disputed_stamp_released` (the stamp check) and `enroll_hold_skipped_on_recovery`
/// (in `open`: the hold of the recovery paths) are the ones that turn them off.
///
/// Implements planning/architecture.md section 7.5 ("The group withholds the wrap until
/// `hold_until`"), section 5.9 and section 12 E2.
///
/// Caller: none yet. Seam: ENROLL-WIRE (S9c's `wraps_releasable` and `device_enroll_finish` ask
/// first, then call `mark_released` before they serve a `member_key` wrap), S47b.
pub fn release(r: &Request, now: Stamp, stamp: StampCheck) -> Release {
    match r.state() {
        RequestState::Cancelled { .. } => return Release::Withheld(Reason::Cancelled),
        RequestState::Expired => return Release::Withheld(Reason::Expired),
        RequestState::Released { at } => return Release::AlreadyReleased { at },
        RequestState::Open => {}
    }
    match stamp {
        StampCheck::Agreed => {}
        StampCheck::Disputed => {
            if !azlin_seams::mutant!("enroll_disputed_stamp_released") {
                return Release::Withheld(Reason::StampDisputed);
            }
        }
    }
    if now > r.expires_at() {
        return Release::Withheld(Reason::Expired);
    }
    if now < r.hold_until() {
        return Release::Withheld(Reason::HoldNotElapsed {
            hold_until: r.hold_until(),
        });
    }
    let rule = r.path().rule();
    for kind in rule.evidence {
        if !satisfied(r, *kind) {
            return Release::Withheld(Reason::EvidenceMissing(*kind));
        }
    }
    Release::Granted(ReleaseGrant {
        request: r.id(),
        at: now,
    })
}

/// What stopped standing between the opening of a request and the moment its key goes out.
///
/// Implements planning/architecture.md section 7.5 ("Any admin or any of the person's devices can
/// cancel") and section 7.6 (the holders are admins): the standing of the ones who act is judged
/// when the key goes out, not only when they acted.
///
/// Caller: `mark_released` (as `ReleaseRefusal::NoLongerStands`). Seam: S9c/ENROLL-WIRE.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StandingLost {
    /// The device that approved is no longer an active device of the person.
    Approver,
    /// The admins who signed the opening entry, the ones who still are active admins, are fewer
    /// than the quorum the entry needed.
    Signers {
        /// The different admins who still stand.
        have: u8,
        /// The different admins the entry needed.
        need: u8,
    },
    /// Nobody who confirmed the fingerprint phrase still has the standing to.
    Confirmer,
    /// The escrow opening is not complete any more, counting only the holders who still stand.
    Escrow(Shortfall),
}

/// Why a grant is not taken.
///
/// Implements planning/architecture.md section 7.5 ("Any admin or any of the person's devices can
/// cancel"): a cancel that came first wins; and section 12 E2.
///
/// Caller: `mark_released`. Seam: S9c/ENROLL-WIRE (does not serve the wrap on an error).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ReleaseRefusal {
    /// The facts are those of another person than the request.
    #[error(transparent)]
    Facts(#[from] crate::request::FactsRefusal),
    /// The leader's stamp is disputed: no key goes out on it.
    #[error("the leader's stamp is disputed: no key goes out on it")]
    StampDisputed,
    /// The grant is for another request.
    #[error("the grant is for another request")]
    WrongRequest,
    /// The key was released already: no second permission to serve is made (the group serves a
    /// device that lost the answer from its own record of the device, not from a second grant).
    #[error("the key was released already: no second permission is made")]
    AlreadyReleased {
        /// The stamp of the release.
        at: Stamp,
    },
    /// The request was cancelled or expired since the grant was made: no wrap goes out.
    #[error("the request is not open any more: no wrap goes out")]
    NotOpen,
    /// The request is past its expiry at the stamp of this call: the grant is stale.
    #[error("the request has expired: the grant is stale")]
    Expired,
    /// Somebody who stood when the request was opened or completed does not stand any more.
    #[error("somebody the request depends on does not stand any more: {0:?}")]
    NoLongerStands(StandingLost),
}

/// Whether the ones the request depends on still stand in these facts: the opener (the device that
/// approved, the admins who signed), the one who confirmed the phrase, the escrow holders.
fn standing(r: &Request, facts: &PersonFacts) -> Result<(), StandingLost> {
    match r.opened_by() {
        OpenedBy::PersonDevice(approver) => {
            if !facts.active_devices.contains(approver) {
                return Err(StandingLost::Approver);
            }
        }
        OpenedBy::Admins(signers) => {
            let standing: Vec<Signatory> = signers
                .iter()
                .filter(|signer| facts.admins.contains(&signer.member))
                .copied()
                .collect();
            let have = distinct_people(&standing);
            let need = r.terms.quorum_need;
            if have < need {
                return Err(StandingLost::Signers { have, need });
            }
        }
        OpenedBy::Person | OpenedBy::Helpdesk(_) => {}
    }
    if r.path()
        .rule()
        .evidence
        .contains(&EvidenceKind::FingerprintPhrase)
    {
        let stands = r.evidence.iter().any(
            |held| matches!(held, Evidence::PhraseConfirmed(who) if confirmer_stands(*who, facts)),
        );
        if !stands {
            return Err(StandingLost::Confirmer);
        }
    }
    if let Some(short) = shortfall_for(r, facts) {
        return Err(StandingLost::Escrow(short));
    }
    Ok(())
}

/// Takes a grant: the request becomes `Released` and the answer is [`Served`], the permission to
/// serve the wrap. The facts are those of the moment, `now` is the stamp of the call and `stamp`
/// the check of it (a disputed stamp serves nothing): the request must still be open and not past
/// its expiry, and everybody it depends on must still stand (see [`StandingLost`]). A request that
/// is already released is refused (`AlreadyReleased`): a second grant of it makes no second
/// permission. A request that was cancelled or expired since the grant was made is refused too, and
/// the wrap stays withheld. A refusal changes nothing: the request stays open and is judged again
/// with the facts of the next moment.
///
/// Implements planning/architecture.md section 7.5 (notice and hold) and section 12 E2 ("an
/// existing device sees every request and can veto it": the veto that came first wins).
///
/// Caller: none yet. Seam: ENROLL-WIRE (S9c serves the wrap named by the `Served` this call
/// returned, and by nothing else).
pub fn mark_released(
    r: &mut Request,
    facts: &PersonFacts,
    grant: ReleaseGrant,
    now: Stamp,
    stamp: StampCheck,
) -> Result<Served, ReleaseRefusal> {
    facts.check_for(r)?;
    if stamp != StampCheck::Agreed {
        return Err(ReleaseRefusal::StampDisputed);
    }
    if grant.request != r.id() {
        return Err(ReleaseRefusal::WrongRequest);
    }
    match r.state() {
        RequestState::Open => {}
        RequestState::Released { at } => return Err(ReleaseRefusal::AlreadyReleased { at }),
        RequestState::Cancelled { .. } | RequestState::Expired => {
            return Err(ReleaseRefusal::NotOpen)
        }
    }
    if now > r.expires_at() {
        return Err(ReleaseRefusal::Expired);
    }
    standing(r, facts).map_err(ReleaseRefusal::NoLongerStands)?;
    r.state = RequestState::Released { at: grant.at };
    let notices = Notices::one(NoticeKind::Released, r, facts);
    Ok(Served {
        request: r.id(),
        person: r.person(),
        new_device: r.new_device(),
        path: r.path(),
        wrap: r.path().rule().wrap,
        at: grant.at,
        notices,
    })
}

/// Closes a request that nobody released in time: when `now` is past its expiry the request
/// becomes `Expired` and the answer is `true`. A stamp that is not agreed expires nothing (a clock
/// that jumped ahead must not kill the requests of the people), and neither does a stamp that
/// went back.
///
/// Implements planning/architecture.md section 7.5 and section 1.4 rule 4 (a request lives for a
/// bounded time; the plan names no expiry: drift row of the lane).
///
/// Caller: none yet. Seam: ENROLL-WIRE (S9c's `sweep_holds` calls it for each pending request).
pub fn expire(r: &mut Request, now: Stamp, stamp: StampCheck) -> bool {
    if stamp != StampCheck::Agreed {
        return false;
    }
    if r.state() == RequestState::Open && now > r.expires_at() {
        r.state = RequestState::Expired;
        return true;
    }
    false
}
