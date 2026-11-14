//! The veto: who may cancel a request (planning/architecture.md section 7.5 "Any admin or any of
//! the person's devices can cancel", "the person or any other admin can object", section 12 E2
//! "an existing device sees every request and can veto it"; build unit S17, lane W1-S17e).
//!
//! WHO CAN CANCEL: any active admin, and any ACTIVE device of the person the request is about.
//! WHO CANNOT: helpdesk ("Helpdesk may only open a recovery request and check the fingerprint
//! phrase with the person out of band": it cannot cancel, and cannot approve alone), a PENDING
//! device (not even the one the request is for: a thief who gets a device enrolled must not be
//! able to cancel the request of the person who tries to get the key back), a suspended or
//! removed admin (not in the facts' list of admins), and a device that is not an active device of
//! this person.
//!
//! The caller names WHAT the signer of the cancel entry is ([`Canceller`], mapped from the team
//! state by the glue), and the crate checks the claim against the facts it is given: an admin must
//! be in `facts.admins` and a device must be in `facts.active_devices`. Cancelling is the safe
//! direction (it withholds), so it never depends on the clock: no stamp refuses a cancel, and the
//! facts are asked for nothing but to be the facts of the person of the request (a veto is never
//! refused for a mistake in the bookkeeping of their lists). A request that was already released
//! cannot be cancelled (the key is out: the answer is to remove the device); one that was
//! cancelled or expired stays so.

use azlin_seams::{DeviceId, MemberId, Stamp};

use crate::notices::{NoticeKind, Notices};
use crate::request::{PersonFacts, Request, RequestState};

/// What the signer of a cancel entry is, as the team state says.
///
/// Implements planning/architecture.md section 7.5 (any admin or any of the person's devices can
/// cancel; helpdesk cannot) and section 5.3 (`reg_device.state`: pending, active, revoked).
///
/// Caller: `cancel`, `RequestState::Cancelled`. Seam: TEAMLOG-APPLY (maps the signer of an
/// `enroll_cancel` or `escrow_open_object` entry), S47b (the veto column of the matrix).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Canceller {
    /// An admin or owner (checked: the member must be in the facts' list of active admins).
    Admin(MemberId),
    /// A device of the person that is active (checked: it must be in the facts' list of the
    /// person's active devices).
    PersonDevice(DeviceId),
    /// A helpdesk member: never cancels.
    Helpdesk(MemberId),
    /// A device that is pending, or revoked: never cancels.
    PendingDevice(DeviceId),
}

/// Why a cancel is refused. A refusal changes nothing.
///
/// Implements planning/architecture.md section 7.5 and section 12 E2.
///
/// Caller: `cancel`. Seam: TEAMLOG-APPLY (refuses the entry with the reason).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum CancelRefusal {
    /// The facts are those of another person than the request.
    #[error(transparent)]
    Facts(#[from] crate::request::FactsRefusal),
    /// The key was already released: the request cannot be cancelled (remove the device).
    #[error("the key was already released")]
    AlreadyReleased,
    /// The request was already cancelled.
    #[error("the request was already cancelled")]
    AlreadyCancelled,
    /// The request expired.
    #[error("the request has expired")]
    Expired,
    /// The member is not an active admin of the account (a suspended or removed admin, or no admin
    /// at all).
    #[error("{member} is not an active admin")]
    NotAnAdmin {
        /// The member who claimed to be an admin.
        member: MemberId,
    },
    /// The device is not an active device of the person.
    #[error("{device} is not an active device of the person")]
    NotAnActiveDevice {
        /// The device that claimed to be one.
        device: DeviceId,
    },
    /// Helpdesk may open a recovery request and check the phrase: it cannot cancel.
    #[error("helpdesk cannot cancel a request")]
    HelpdeskCannotCancel,
    /// A pending device cannot cancel, not even the request it belongs to.
    #[error("a pending device cannot cancel a request")]
    PendingDeviceCannotCancel,
}

/// Cancels an open request: the request becomes `Cancelled` and the notices say so to the same
/// audience as the opening. `at` is the leader's stamp of the entry and is recorded as it is.
///
/// Implements planning/architecture.md section 7.5 ("Any admin or any of the person's devices can
/// cancel"; "the person or any other admin can object") and section 12 E2. The guards
/// `enroll_cancel_by_helpdesk` and `enroll_pending_device_may_cancel` turn two of its refusals off.
///
/// Caller: none yet. Seam: TEAMLOG-APPLY (the `enroll_cancel` and `escrow_open_object` ops of
/// 5.9), then the glue calls `OpenLimits::closed`; S47b.
pub fn cancel(
    r: &mut Request,
    facts: &PersonFacts,
    by: Canceller,
    at: Stamp,
) -> Result<Notices, CancelRefusal> {
    facts.check_for(r)?;
    match r.state() {
        RequestState::Open => {}
        RequestState::Cancelled { .. } => return Err(CancelRefusal::AlreadyCancelled),
        RequestState::Released { .. } => return Err(CancelRefusal::AlreadyReleased),
        RequestState::Expired => return Err(CancelRefusal::Expired),
    }
    match by {
        Canceller::Admin(member) => {
            if !facts.admins.contains(&member) {
                return Err(CancelRefusal::NotAnAdmin { member });
            }
        }
        Canceller::PersonDevice(device) => {
            if !facts.active_devices.contains(&device) {
                return Err(CancelRefusal::NotAnActiveDevice { device });
            }
        }
        Canceller::Helpdesk(_) => {
            if !azlin_seams::mutant!("enroll_cancel_by_helpdesk") {
                return Err(CancelRefusal::HelpdeskCannotCancel);
            }
        }
        Canceller::PendingDevice(_) => {
            if !azlin_seams::mutant!("enroll_pending_device_may_cancel") {
                return Err(CancelRefusal::PendingDeviceCannotCancel);
            }
        }
    }
    r.state = RequestState::Cancelled { by, at };
    Ok(Notices::one(NoticeKind::Cancelled, r, facts))
}
