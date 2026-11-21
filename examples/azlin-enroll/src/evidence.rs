//! The evidence of a request: what a path needs before a key may be released, and the rules for
//! each piece (planning/architecture.md section 7.5 "Enrollment and recovery that an attacker
//! cannot use", section 5.5 "Recovery", section 5.11 "attestations"; build unit S17, lane
//! W1-S17e).
//!
//! THE EVIDENCE ARE FACTS, NOT PROOFS. A signature, an ID token, a passkey assertion or a contact's
//! reply is verified by the unit that owns its format (S17d webauthn, S17g idtoken, S17f escrow,
//! the glue); this crate takes the verified fact and decides what the facts add up to. It never
//! sees a secret and verifies no signature.
//!
//! EVERY FACT NAMES THE DEVICE IT WAS VERIFIED FOR. The attestation of the identity service names a
//! device (5.11), and so does every other piece of evidence here: `add_evidence` refuses a fact
//! that was verified for another device than the one the request is for, so a proof made for one
//! request cannot be moved into another request of the same person by a mix-up of the glue.
//!
//! The rules of the pieces:
//!
//! - A fresh SSO attestation ([`Evidence::Sso`]) is necessary and never sufficient (section 7.5):
//!   it must name the person of the request ("the approver checks its subject": the Bitwarden flaw
//!   of 2026, CVE-2026-60104, was an approval request whose email was not checked against its
//!   sender) and be young enough ([`SSO_FRESH`], or the policy's shorter `sso_max_age`). It is
//!   judged when it is presented, not at the release: the hold is 48 hours.
//! - Device evidence counts only as an MDM certificate tied to the company's inventory; a TPM or
//!   Secure Enclave attestation proves genuine hardware, not a company device, and is refused as
//!   device evidence ([`DeviceEvidenceKind`]).
//! - The contacts' shares need two of the three contacts, counted by distinct slot
//!   ([`CONTACT_SHARES_NEEDED`]).
//! - A passkey assertion needs user verification.
//! - The fingerprint phrase is confirmed by the device that approved an own-device request, or by
//!   one of the admins who signed a quorum enrollment, never by anybody else; the standing of the
//!   one who confirmed is judged again when the key goes out (`mark_released`).
//!
//! A piece of evidence that the path's row of `PATHS` does not list is refused, and a second piece
//! of a kind adds nothing (the first stays), so the evidence of a request is bounded
//! ([`MAX_EVIDENCE`]). A call with a disputed stamp adds nothing: the age of an attestation is a
//! time-dependent fact.

use azlin_seams::{DeviceId, MemberId, Span, Stamp};

use crate::path::Path;
use crate::release::StampCheck;
use crate::request::{Liveness, OpenedBy, PersonFacts, Request};

/// The most pieces of evidence one request can keep (section 1.4 rule 4): eight, by construction.
/// A request keeps one piece of each kind (the phrase, the SSO attestation, the recovery code's
/// proof, the passkey assertion, the device evidence) and up to three shares of the contacts, and
/// `add_evidence` ignores every further piece of a kind, so the list cannot grow past it.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up
/// front).
///
/// Caller: the property test of the crate (it asserts that no request holds more).
pub const MAX_EVIDENCE: usize = 8;

/// How young a fresh SSO attestation must be when it is presented: ten minutes (an estimate, drift
/// row of the lane; the plan says "fresh" and gives no number). A policy with a shorter
/// `sso_max_age` makes it shorter; a longer one does NOT make it longer (the policy's maximum age
/// exists to keep sessions through identity-service outages, which a recovery does not need).
///
/// Implements planning/architecture.md section 7.5 ("a fresh SSO attestation for the person") and
/// section 5.10 (`sso_max_age_s`).
///
/// Caller: `open` (fixes the age of the request).
pub const SSO_FRESH: Span = Span::minutes(10);

/// The shares of the contacts that give the recovery code back: two ("2-of-3 Shamir shares to
/// contacts").
///
/// Implements planning/architecture.md section 5.5 ("its 2-of-3 Shamir shares to contacts").
///
/// Caller: `satisfied` (the contact-shares evidence).
pub const CONTACT_SHARES_NEEDED: usize = 2;

/// The shares made for the contacts: three.
///
/// Implements planning/architecture.md section 5.5 ("its 2-of-3 Shamir shares to contacts").
///
/// Caller: `ContactSlot::new`.
pub const CONTACT_SHARES_TOTAL: u8 = 3;

/// The kinds of evidence of the table of paths.
///
/// Implements planning/architecture.md section 7.5 (the requirements of each path) and section 5.5.
///
/// Caller: `PathRule::evidence`, `Evidence::kind`, `Reason::EvidenceMissing`. Seam: S47b (the
/// matrix removes one kind at a time).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum EvidenceKind {
    /// The fingerprint phrase compared by the approver.
    FingerprintPhrase,
    /// The opening entry was signed by enough different admins (the policy's quorum); judged when
    /// the request is opened (and again for their standing when the key goes out).
    AdminQuorum,
    /// A fresh SSO attestation for the person.
    SsoAttestation,
    /// Escrow opened by the threshold: the contributions of the escrow holders (see
    /// `escrow_open`).
    EscrowThreshold,
    /// Proof that the recovery code is known.
    RecoveryCodeProof,
    /// Two of the three contacts released their share.
    ContactShares,
    /// A passkey assertion with user verification.
    PasskeyAssertion,
    /// Device evidence of the new device: accepted whichever path the request is on, and listed by
    /// no path's row (no release waits for it).
    DeviceEvidence,
}

/// One of the three contacts that hold a share of the recovery code, by the number of the share.
///
/// Implements planning/architecture.md section 5.5 ("2-of-3 Shamir shares to contacts").
///
/// Caller: `Evidence::ContactShare`. Seam: the glue maps a verified contact reply to its slot.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContactSlot(u8);

/// A number that is none of the contacts' slots (1 to 3).
///
/// Implements planning/architecture.md section 5.5.
///
/// Caller: `ContactSlot::new`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("a contact slot is 1 to {}, not {slot}", CONTACT_SHARES_TOTAL)]
pub struct SlotOutOfRange {
    /// The refused number.
    pub slot: u8,
}

impl ContactSlot {
    /// The slot with this number, 1 to [`CONTACT_SHARES_TOTAL`].
    pub fn new(slot: u8) -> Result<ContactSlot, SlotOutOfRange> {
        if slot == 0 || slot > CONTACT_SHARES_TOTAL {
            return Err(SlotOutOfRange { slot });
        }
        Ok(ContactSlot(slot))
    }

    /// The number of the slot.
    pub const fn get(self) -> u8 {
        self.0
    }
}

/// The kinds of device evidence a new device can bring.
///
/// Implements planning/architecture.md section 7.5 ("Device evidence counts only as an MDM
/// certificate tied to the company's inventory. A TPM or Secure Enclave attestation proves
/// genuine hardware, not a company device") and section 5.3 (`reg_device.evidence`).
///
/// Caller: `Evidence::Device`, `add_evidence`. Seam: S9c (`reg_device.evidence`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum DeviceEvidenceKind {
    /// An MDM certificate tied to the company's inventory: the one device evidence.
    MdmCertificate,
    /// A TPM attestation: genuine hardware, not a company device.
    TpmAttestation,
    /// A Secure Enclave attestation: genuine hardware, not a company device.
    SecureEnclaveAttestation,
}

impl DeviceEvidenceKind {
    /// Whether this is evidence of a company device: only the MDM certificate is.
    pub const fn counts_as_company_device(self) -> bool {
        matches!(self, DeviceEvidenceKind::MdmCertificate)
    }
}

/// Who confirmed the fingerprint phrase: the device that approved an own-device request, or one of
/// the admins who signed a quorum enrollment.
///
/// Implements planning/architecture.md section 7.4 ("compares the fingerprint phrase") and section
/// 7.5 ("check the fingerprint phrase with the person out of band").
///
/// Caller: `Evidence::PhraseConfirmed`, `add_evidence`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Confirmer {
    /// The approving device of an own-device request.
    Device(DeviceId),
    /// An admin who signed the quorum enrollment.
    Member(MemberId),
}

/// One verified fact about a request. Every fact but the phrase names the device it was verified
/// for, and `add_evidence` refuses one that names another device than the request's.
///
/// Implements planning/architecture.md section 7.5, section 5.5 (what each path needs) and section
/// 5.11 (an attestation names the device).
///
/// Caller: `add_evidence`, `Request::evidence`. Seam: S17d/S17g/S17f and the glue build them from
/// what they verified.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Evidence {
    /// The approver compared the phrases of the two devices and they were the same
    /// (`confirm_phrase` succeeded): the fact of the approver's own signed entry.
    PhraseConfirmed(Confirmer),
    /// A fresh SSO attestation: the member it names, the device it was made for and when the
    /// identity service issued it.
    Sso {
        /// The member the attestation is for.
        subject: MemberId,
        /// The device the attestation was made for.
        device: DeviceId,
        /// When the identity service issued it, read on the leader's clock: the time the
        /// attestation carries, never the time it arrived.
        issued_at: Stamp,
    },
    /// The recovery code was proved (the key derived from it signed the request).
    RecoveryCodeProof {
        /// The device the proof was made for.
        device: DeviceId,
    },
    /// A contact released their share.
    ContactShare {
        /// The contact.
        slot: ContactSlot,
        /// The device the share was released to.
        device: DeviceId,
    },
    /// A passkey assertion, with or without user verification.
    PasskeyAssertion {
        /// The device the assertion was made for.
        device: DeviceId,
        /// Whether the assertion had user verification (a PIN or a biometric).
        user_verified: bool,
    },
    /// Device evidence of the new device.
    Device {
        /// The device the evidence is about.
        device: DeviceId,
        /// The kind of evidence.
        kind: DeviceEvidenceKind,
    },
}

impl Evidence {
    /// The kind of this piece of evidence.
    pub const fn kind(&self) -> EvidenceKind {
        match self {
            Evidence::PhraseConfirmed(_) => EvidenceKind::FingerprintPhrase,
            Evidence::Sso { .. } => EvidenceKind::SsoAttestation,
            Evidence::RecoveryCodeProof { .. } => EvidenceKind::RecoveryCodeProof,
            Evidence::ContactShare { .. } => EvidenceKind::ContactShares,
            Evidence::PasskeyAssertion { .. } => EvidenceKind::PasskeyAssertion,
            Evidence::Device { .. } => EvidenceKind::DeviceEvidence,
        }
    }

    /// The device the fact was verified for; `None` for the phrase, which is the fact of the
    /// approver's own entry.
    pub const fn device(&self) -> Option<DeviceId> {
        match self {
            Evidence::PhraseConfirmed(_) => None,
            Evidence::Sso { device, .. }
            | Evidence::RecoveryCodeProof { device }
            | Evidence::ContactShare { device, .. }
            | Evidence::PasskeyAssertion { device, .. }
            | Evidence::Device { device, .. } => Some(*device),
        }
    }
}

/// Why a piece of evidence is refused. A refusal changes nothing.
///
/// Implements planning/architecture.md section 7.5 (each rule of the evidence).
///
/// Caller: `add_evidence`. Seam: S9c/ENROLL-WIRE (maps it to a refusal of the entry).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum EvidenceRefusal {
    /// The leader's stamp is disputed: nothing is added on it.
    #[error("the leader's stamp is disputed: no evidence is added on it")]
    StampDisputed,
    /// The request is cancelled, released or expired.
    #[error("the request is not open any more")]
    NotOpen,
    /// The request is past its expiry.
    #[error("the request has expired")]
    Expired,
    /// The path does not ask for this kind of evidence.
    #[error("{kind:?} is no evidence of the path {path}")]
    NotForThisPath {
        /// The path of the request.
        path: Path,
        /// The kind of the refused piece.
        kind: EvidenceKind,
    },
    /// The fact was verified for another device than the one the request is for.
    #[error("the fact was verified for another device than the request is for")]
    WrongDevice,
    /// The phrase was not confirmed by the device or admin that opened the request.
    #[error("the phrase was not confirmed by the one who opened the request")]
    WrongConfirmer,
    /// The SSO attestation names another person (CVE-2026-60104).
    #[error("the SSO attestation is for another person than the request")]
    SsoSubjectMismatch,
    /// The SSO attestation is older than the maximum age.
    #[error("the SSO attestation is too old")]
    SsoStale,
    /// The SSO attestation was issued after the stamp of this call.
    #[error("the SSO attestation was issued after now")]
    SsoFromTheFuture,
    /// The passkey assertion had no user verification.
    #[error("the passkey assertion had no user verification")]
    UserVerificationMissing,
    /// A TPM or Secure Enclave attestation: genuine hardware, not a company device.
    #[error("{0:?} proves genuine hardware, not a company device")]
    NotDeviceEvidence(DeviceEvidenceKind),
}

/// Adds a verified piece of evidence to an open request. A piece the same as one the request holds,
/// and a second piece of a kind (but for the contacts' shares), are accepted and change nothing.
/// `now` is the leader's stamp of the call and `stamp` the check of it: a disputed stamp adds
/// nothing. The request is judged at `now`, but never earlier than its own `opened_at`, so a stamp
/// that went back gives no younger attestation.
///
/// Implements planning/architecture.md section 7.5 (a fresh SSO attestation for the person, its
/// subject checked; device evidence only as an MDM certificate), section 5.5 and section 5.11 (the
/// attestation names the device). The guard `enroll_sso_subject_unchecked` turns the subject check
/// off.
///
/// Caller: none yet. Seam: ENROLL-WIRE (S9c's enrollment handlers call it for each verified piece
/// of an entry), S47b (the matrix).
pub fn add_evidence(
    r: &mut Request,
    e: Evidence,
    now: Stamp,
    stamp: StampCheck,
) -> Result<(), EvidenceRefusal> {
    if stamp != StampCheck::Agreed {
        return Err(EvidenceRefusal::StampDisputed);
    }
    let at = r.anchored(now);
    match r.liveness(at) {
        Liveness::Live => {}
        Liveness::Closed => return Err(EvidenceRefusal::NotOpen),
        Liveness::Expired => return Err(EvidenceRefusal::Expired),
    }
    let kind = e.kind();
    if kind != EvidenceKind::DeviceEvidence && !r.path().rule().evidence.contains(&kind) {
        return Err(EvidenceRefusal::NotForThisPath {
            path: r.path(),
            kind,
        });
    }
    if let Some(device) = e.device() {
        if device != r.new_device() {
            return Err(EvidenceRefusal::WrongDevice);
        }
    }
    match e {
        Evidence::PhraseConfirmed(who) => check_confirmer(r, who)?,
        Evidence::Sso {
            subject, issued_at, ..
        } => {
            if subject != r.person() && !azlin_seams::mutant!("enroll_sso_subject_unchecked") {
                return Err(EvidenceRefusal::SsoSubjectMismatch);
            }
            check_fresh(issued_at, at, r.sso_max_age())?;
        }
        Evidence::PasskeyAssertion { user_verified, .. } => {
            if !user_verified {
                return Err(EvidenceRefusal::UserVerificationMissing);
            }
        }
        Evidence::RecoveryCodeProof { .. } | Evidence::ContactShare { .. } => {}
        Evidence::Device { kind, .. } => {
            if !kind.counts_as_company_device() {
                return Err(EvidenceRefusal::NotDeviceEvidence(kind));
            }
        }
    }
    if r.evidence.contains(&e) {
        return Ok(());
    }
    if kind != EvidenceKind::ContactShares && r.evidence.iter().any(|held| held.kind() == kind) {
        return Ok(());
    }
    r.evidence.push(e);
    Ok(())
}

/// The phrase is confirmed by the one who opened the request: the approving device of an
/// own-device request, one of the admins who signed a quorum enrollment.
fn check_confirmer(r: &Request, who: Confirmer) -> Result<(), EvidenceRefusal> {
    let allowed = match (r.opened_by(), who) {
        (OpenedBy::PersonDevice(approver), Confirmer::Device(device)) => *approver == device,
        (OpenedBy::Admins(signers), Confirmer::Member(member)) => {
            signers.iter().any(|signer| signer.member == member)
        }
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(EvidenceRefusal::WrongConfirmer)
    }
}

/// The attestation was issued at `issued_at` and is judged at `now`: not after it, and not older
/// than `max_age` (an age of exactly `max_age` is still fresh).
fn check_fresh(issued_at: Stamp, now: Stamp, max_age: Span) -> Result<(), EvidenceRefusal> {
    match now.elapsed_since(issued_at) {
        None => Err(EvidenceRefusal::SsoFromTheFuture),
        Some(age) if age > max_age => Err(EvidenceRefusal::SsoStale),
        Some(_) => Ok(()),
    }
}

/// Whether the one who confirmed the phrase still has the standing to: a device that is still an
/// active device of the person, an admin who is still an active admin.
pub(crate) fn confirmer_stands(who: Confirmer, facts: &PersonFacts) -> bool {
    match who {
        Confirmer::Device(device) => facts.active_devices.contains(&device),
        Confirmer::Member(member) => facts.admins.contains(&member),
    }
}

/// Whether the request holds this kind of evidence, complete: the test of `release` for every kind
/// of the path's row.
pub(crate) fn satisfied(r: &Request, kind: EvidenceKind) -> bool {
    match kind {
        EvidenceKind::FingerprintPhrase
        | EvidenceKind::SsoAttestation
        | EvidenceKind::RecoveryCodeProof
        | EvidenceKind::PasskeyAssertion
        | EvidenceKind::DeviceEvidence => r.evidence.iter().any(|held| held.kind() == kind),
        EvidenceKind::ContactShares => {
            let mut slots: Vec<ContactSlot> = Vec::new();
            for held in &r.evidence {
                if let Evidence::ContactShare { slot, .. } = held {
                    if !slots.contains(slot) {
                        slots.push(*slot);
                    }
                }
            }
            slots.len() >= CONTACT_SHARES_NEEDED
        }
        // Checked when the request was opened: only an opening entry signed by enough admins
        // makes a request on a path that lists it (the openers of such a path are admins only).
        EvidenceKind::AdminQuorum => matches!(r.opened_by(), OpenedBy::Admins(_)),
        EvidenceKind::EscrowThreshold => crate::escrow_open::shortfall(r).is_none(),
    }
}
