//! The notices of a request: who must be told, as data (planning/architecture.md section 7.5
//! "Notice and hold", section 5.9 "Notices come from the metadata group", section 12 E2; build
//! unit S17, lane W1-S17e).
//!
//! "Every enrollment emits `log_appended` from the metadata group to every admin and to the
//! person's devices." This crate does not send anything: `open`, `cancel` and `mark_released`
//! RETURN the notices the group must emit, with the audience read from the facts of that moment.
//! A notice that is lost changes no state: the request stays open, and every device and admin
//! still finds it with `pending_for_person` and `pending` when it syncs (E2: "an existing device
//! sees every request and can veto it").
//!
//! For an escrow opening the audience is the same: "notice to the person (if active) and every
//! admin" (section 7.5): a person with no active device has no device to tell, and the admins are
//! told.

use azlin_seams::{DeviceId, MemberId};

use crate::path::Path;
use crate::request::{PersonFacts, Request, RequestId};

/// What happened to the request.
///
/// Implements planning/architecture.md section 5.9 (`log_appended`: op and subject) and section
/// 7.5 (every enrollment, every cancel).
///
/// Caller: `Notices::one` (the notices `open`, `cancel` and `mark_released` return). Seam: S33
/// (derives events from the notices), S9c (emits `log_appended`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// A request was opened on this path.
    Opened {
        /// The path of the request.
        path: Path,
    },
    /// An admin or an active device of the person cancelled the request.
    Cancelled,
    /// The key was released: the group may serve the wrap.
    Released,
}

/// Who a notice goes to: every admin and every active device of the person, as they were when the
/// notice was made.
///
/// Implements planning/architecture.md section 7.5 ("to every admin and to the person's devices")
/// and section 5.9 ("to every admin and to the subject's devices").
///
/// Caller: `Notice`. Seam: S9c/S33 (the group delivers to exactly these).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Audience {
    /// Every active admin.
    pub admins: Vec<MemberId>,
    /// Every active device of the person.
    pub devices: Vec<DeviceId>,
}

/// One notice: `log_appended` for a request, to an audience.
///
/// Implements planning/architecture.md section 5.9 and section 7.5 (notice and hold).
///
/// Caller: `Notices`. Seam: S9c/S33 (emit it over `watch` and push).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notice {
    /// What happened.
    pub kind: NoticeKind,
    /// The request it is about.
    pub request: RequestId,
    /// The person the request is about (the subject of the entry).
    pub person: MemberId,
    /// Who is told.
    pub audience: Audience,
}

/// The notices one call makes: one for each opening, cancel and release; a call that is refused
/// returns the refusal and makes none.
///
/// Implements planning/architecture.md section 7.5 ("Every enrollment emits `log_appended`") and
/// section 5.9.
///
/// Caller: `open`, `cancel` (their return value), `Served` (the notices of `mark_released`). Seam:
/// S9c/S33 (emit them).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notices(Vec<Notice>);

impl Notices {
    /// One notice of this kind about `request`, to the admins and the devices of `facts`.
    pub(crate) fn one(kind: NoticeKind, request: &Request, facts: &PersonFacts) -> Notices {
        Notices(vec![Notice {
            kind,
            request: request.id(),
            person: request.person(),
            audience: Audience {
                admins: facts.admins.clone(),
                devices: facts.active_devices.clone(),
            },
        }])
    }

    /// The notices, in the order they were made.
    pub fn iter(&self) -> core::slice::Iter<'_, Notice> {
        self.0.iter()
    }

    /// How many notices.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there is no notice.
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }

    /// Every admin any notice goes to, without repeats.
    pub fn admins(&self) -> Vec<MemberId> {
        let mut out: Vec<MemberId> = Vec::new();
        for notice in &self.0 {
            for admin in &notice.audience.admins {
                if !out.contains(admin) {
                    out.push(*admin);
                }
            }
        }
        out
    }

    /// Every device any notice goes to, without repeats.
    pub fn devices(&self) -> Vec<DeviceId> {
        let mut out: Vec<DeviceId> = Vec::new();
        for notice in &self.0 {
            for device in &notice.audience.devices {
                if !out.contains(device) {
                    out.push(*device);
                }
            }
        }
        out
    }

    /// The notices as a list.
    pub fn into_vec(self) -> Vec<Notice> {
        self.0
    }
}

impl<'a> IntoIterator for &'a Notices {
    type Item = &'a Notice;
    type IntoIter = core::slice::Iter<'a, Notice>;

    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}
