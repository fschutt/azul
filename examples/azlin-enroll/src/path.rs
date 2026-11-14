//! The seven paths of enrollment and recovery as DATA: for each path the hold it waits, the
//! evidence it needs, who may open it, and the gate that policy and facts put before it
//! (planning/architecture.md section 7.5 "Enrollment and recovery that an attacker cannot use",
//! section 5.5 "Recovery", section 7.6 "Escrow and the key agent"; build unit S17, lane
//! W1-S17e).
//!
//! [`PATHS`] is the table of section 7.5 as a constant. The decisions of the crate (`open`,
//! `add_evidence`, `release`) read this table and nowhere else: a path's hold, its evidence and
//! its openers are changed here and nowhere else, and the escalation matrix of lane S47b is
//! generated from this table AND from the document, so a row edited in one of them shows.
//!
//! How the seven paths cover the plan (the owner confirms the list: drift row of the lane):
//!
//! | path | where the plan has it | release |
//! |---|---|---|
//! | [`Path::OwnDevice`] | 7.4 and the first row of 7.5: the person's own device scans the QR code, compares the fingerprint phrase and wraps the member key | no hold |
//! | [`Path::QuorumEnrollment`] | 5.10 `device_approval` "self or quorum", `admin_quorum` "quorum enrollments" | the enrollment hold |
//! | [`Path::RecoveryCode`] | 5.5: the recovery code wraps the member key (envelope alg 3) | the enrollment hold |
//! | [`Path::ContactShares`] | 5.5: 2-of-3 Shamir shares to contacts (alg 5) | the enrollment hold |
//! | [`Path::PasskeyWrap`] | 5.5: the passkey PRF (alg 4), on by default for personal and family | the enrollment hold |
//! | [`Path::EscrowRecovery`] | the second row of 7.5: a fresh SSO attestation, escrow opened by the threshold, the hold | the enrollment hold |
//! | [`Path::EscrowOpen`] | 7.5 and 7.6: a leaver's handover or a legal request | the escrow-opening wait |

use core::fmt;
use core::str::FromStr;

use azlin_seams::UnknownWord;

use crate::evidence::EvidenceKind;
use crate::hold::HoldKind;

/// A way to get a key onto a new device (or, for `EscrowOpen`, to a recipient's device).
///
/// Implements planning/architecture.md section 7.5 (the table of paths) and section 5.5 (the three
/// recovery wraps).
///
/// Caller: `open`, `add_evidence`, `release` (every decision starts from `Path::rule`). Seam: S47b
/// (the matrix is generated from `PATHS`), S9c (ENROLL-WIRE maps `enrolled_by` to a path).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Path {
    /// The person's own existing, active device approves a new device (QR code and fingerprint
    /// phrase); that device wraps the member key; no hold.
    OwnDevice,
    /// An admin quorum approves a new device of a member who has no device to approve with
    /// (`device_approval` allows a quorum); the hold applies. That the member has no device to
    /// approve with is the reason for the path, not a condition the crate checks.
    QuorumEnrollment,
    /// The recovery code (the paper key) wraps the member key; the hold applies.
    RecoveryCode,
    /// Two of the three contacts release their share; the hold applies.
    ContactShares,
    /// The passkey wrap (PRF), when the policy's `passkey_recovery` allows it; the hold applies.
    PasskeyWrap,
    /// A managed member who lost every device (the reason for the path, not a condition the crate
    /// checks): a fresh SSO attestation, escrow opened by the threshold, and the hold.
    EscrowRecovery,
    /// Opening a member's data through escrow (a leaver's handover, a legal request): the
    /// escrow-opening wait.
    EscrowOpen,
}

impl Path {
    /// Every path, in the order of [`PATHS`].
    pub const ALL: [Path; 7] = [
        Path::OwnDevice,
        Path::QuorumEnrollment,
        Path::RecoveryCode,
        Path::ContactShares,
        Path::PasskeyWrap,
        Path::EscrowRecovery,
        Path::EscrowOpen,
    ];

    /// The word of this path: lower-case snake case, the one spelling `FromStr` accepts.
    pub const fn as_str(self) -> &'static str {
        match self {
            Path::OwnDevice => "own_device",
            Path::QuorumEnrollment => "quorum_enrollment",
            Path::RecoveryCode => "recovery_code",
            Path::ContactShares => "contact_shares",
            Path::PasskeyWrap => "passkey_wrap",
            Path::EscrowRecovery => "escrow_recovery",
            Path::EscrowOpen => "escrow_open",
        }
    }

    /// The rule of this path: its row of [`PATHS`].
    pub fn rule(self) -> &'static PathRule {
        match self {
            Path::OwnDevice => &OWN_DEVICE,
            Path::QuorumEnrollment => &QUORUM_ENROLLMENT,
            Path::RecoveryCode => &RECOVERY_CODE,
            Path::ContactShares => &CONTACT_SHARES,
            Path::PasskeyWrap => &PASSKEY_WRAP,
            Path::EscrowRecovery => &ESCROW_RECOVERY,
            Path::EscrowOpen => &ESCROW_OPEN,
        }
    }

    /// Whether this path is a recovery: "the same hold applies to every recovery: the recovery
    /// code, the contacts' shares and the passkey wrap" (section 7.5), and the escrow recovery.
    pub const fn is_recovery(self) -> bool {
        matches!(
            self,
            Path::RecoveryCode | Path::ContactShares | Path::PasskeyWrap | Path::EscrowRecovery
        )
    }

    /// Whether the key is opened through the company's escrow (threshold decryption, section
    /// 7.6): the contributions of the escrow holders decide.
    pub const fn is_escrow(self) -> bool {
        matches!(self, Path::EscrowRecovery | Path::EscrowOpen)
    }

    /// Whether the path enrolls a new device for the person (every path but `EscrowOpen`, which
    /// hands the data to a recipient's device): the person's device limit applies to it.
    pub const fn enrolls_a_device(self) -> bool {
        !matches!(self, Path::EscrowOpen)
    }
}

impl fmt::Display for Path {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Strict: exactly one of the words of [`Path::as_str`].
impl FromStr for Path {
    type Err = UnknownWord;

    fn from_str(text: &str) -> Result<Path, UnknownWord> {
        match text {
            "own_device" => Ok(Path::OwnDevice),
            "quorum_enrollment" => Ok(Path::QuorumEnrollment),
            "recovery_code" => Ok(Path::RecoveryCode),
            "contact_shares" => Ok(Path::ContactShares),
            "passkey_wrap" => Ok(Path::PasskeyWrap),
            "escrow_recovery" => Ok(Path::EscrowRecovery),
            "escrow_open" => Ok(Path::EscrowOpen),
            _ => Err(UnknownWord {
                vocabulary: "enrollment path",
            }),
        }
    }
}

/// Who opens a request, as a kind (the identity of the opener is in `OpenedBy`).
///
/// Implements planning/architecture.md section 7.5 ("Helpdesk may only open a recovery request";
/// the person's own device; an admin quorum) and section 7.6 (the one who asks).
///
/// Caller: `PathRule::openers`, `open`, `OpenLimits` (one ledger for each kind of opener, so that a
/// lower kind never blocks a higher one). Seam: S47b (the matrix reads who may open each path).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OpenerKind {
    /// An active device of the person (the approver of the QR enrollment).
    PersonDevice,
    /// The person themself, from the new device.
    Person,
    /// One or more admins who signed the opening entry.
    Admins,
    /// A helpdesk member: opens a recovery request, nothing else.
    Helpdesk,
}

impl OpenerKind {
    /// Every kind of opener.
    pub const ALL: [OpenerKind; 4] = [
        OpenerKind::PersonDevice,
        OpenerKind::Person,
        OpenerKind::Admins,
        OpenerKind::Helpdesk,
    ];

    /// The word of this kind: lower-case snake case.
    pub const fn as_str(self) -> &'static str {
        match self {
            OpenerKind::PersonDevice => "person_device",
            OpenerKind::Person => "person",
            OpenerKind::Admins => "admins",
            OpenerKind::Helpdesk => "helpdesk",
        }
    }
}

/// The gate that policy and facts put before a path: what must hold at `open`.
///
/// Implements planning/architecture.md section 5.10 (`device_approval`, `passkey_recovery`),
/// section 5.5 (the passkey wrap is on for personal and family, off for business) and section
/// 7.6 (private members and family members are never escrowed).
///
/// Caller: `PathRule::policy_gate`, `open`. Seam: S47b (the matrix varies the policy by gate).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PolicyGate {
    /// Always available.
    Always,
    /// `device_approval` must be "self or quorum".
    DeviceApprovalAllowsQuorum,
    /// `passkey_recovery` must be on.
    PasskeyRecoveryOn,
    /// The member must be managed and the account must have an escrow.
    ManagedMemberWithEscrow,
}

/// Which stored wrap the metadata group serves to the new device when the request is released
/// (the interpretation of the lane's brief: the group withholds `reg_wrap` rows).
///
/// Implements planning/architecture.md section 5.3 (`reg_wrap` kinds `member_key` and `escrow`)
/// and section 5.5 (envelope algorithms 1 and 2 sealed to a device, 3 Argon2id passphrase, 4 PRF,
/// 5 Shamir share, 7 threshold escrow).
///
/// Caller: `PathRule::wrap`, `ReleaseGrant::wrap`. Seam: S9c/ENROLL-WIRE (which row to serve).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WrapKind {
    /// The member key sealed to the new device's key by the approving device (algorithm 1 or 2).
    SealedToDevice,
    /// The member key under the recovery code (algorithm 3).
    RecoveryCode,
    /// The member key under the passkey PRF (algorithm 4).
    Passkey,
    /// The member key under a Shamir share of the recovery code (algorithm 5).
    ContactShare,
    /// The `escrow` wrap opened by threshold decryption (algorithm 7).
    Escrow,
}

/// One row of the table of paths: what a path waits and needs.
///
/// Implements planning/architecture.md section 7.5 (the table), section 5.10 (`enroll_hold_s`,
/// `escrow_open_wait_s`) and section 12 E2.
///
/// Caller: `Path::rule`, `open`, `add_evidence`, `release`. Seam: S47b (generates its matrix from
/// it).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PathRule {
    /// The hold the group withholds the wrap for.
    pub hold: HoldKind,
    /// The evidence a release needs: every kind listed, no release without all of them.
    pub evidence: &'static [EvidenceKind],
    /// Who may open a request on this path.
    pub openers: &'static [OpenerKind],
    /// The gate of policy and facts at `open`.
    pub policy_gate: PolicyGate,
    /// The wrap the group serves on release.
    pub wrap: WrapKind,
}

const OWN_DEVICE: PathRule = PathRule {
    hold: HoldKind::None,
    evidence: &[EvidenceKind::FingerprintPhrase],
    openers: &[OpenerKind::PersonDevice],
    policy_gate: PolicyGate::Always,
    wrap: WrapKind::SealedToDevice,
};

const QUORUM_ENROLLMENT: PathRule = PathRule {
    hold: HoldKind::EnrollHold,
    evidence: &[EvidenceKind::FingerprintPhrase, EvidenceKind::AdminQuorum],
    openers: &[OpenerKind::Admins],
    policy_gate: PolicyGate::DeviceApprovalAllowsQuorum,
    wrap: WrapKind::SealedToDevice,
};

const RECOVERY_CODE: PathRule = PathRule {
    hold: HoldKind::EnrollHold,
    evidence: &[EvidenceKind::RecoveryCodeProof],
    openers: &[OpenerKind::Person],
    policy_gate: PolicyGate::Always,
    wrap: WrapKind::RecoveryCode,
};

const CONTACT_SHARES: PathRule = PathRule {
    hold: HoldKind::EnrollHold,
    evidence: &[EvidenceKind::ContactShares],
    openers: &[OpenerKind::Person],
    policy_gate: PolicyGate::Always,
    wrap: WrapKind::ContactShare,
};

const PASSKEY_WRAP: PathRule = PathRule {
    hold: HoldKind::EnrollHold,
    evidence: &[EvidenceKind::PasskeyAssertion],
    openers: &[OpenerKind::Person],
    policy_gate: PolicyGate::PasskeyRecoveryOn,
    wrap: WrapKind::Passkey,
};

const ESCROW_RECOVERY: PathRule = PathRule {
    hold: HoldKind::EnrollHold,
    evidence: &[EvidenceKind::SsoAttestation, EvidenceKind::EscrowThreshold],
    openers: &[OpenerKind::Person, OpenerKind::Helpdesk],
    policy_gate: PolicyGate::ManagedMemberWithEscrow,
    wrap: WrapKind::Escrow,
};

const ESCROW_OPEN: PathRule = PathRule {
    hold: HoldKind::EscrowOpenWait,
    evidence: &[EvidenceKind::AdminQuorum, EvidenceKind::EscrowThreshold],
    openers: &[OpenerKind::Admins],
    policy_gate: PolicyGate::ManagedMemberWithEscrow,
    wrap: WrapKind::Escrow,
};

/// The table of section 7.5 as data: every path with its rule, in the order of [`Path::ALL`].
///
/// Implements planning/architecture.md section 7.5 (the table of paths), section 5.5 and section
/// 7.6.
///
/// Seam: S47b (`azlin-escalate-e2` generates its matrix from this table and from the document).
pub const PATHS: [(Path, PathRule); 7] = [
    (Path::OwnDevice, OWN_DEVICE),
    (Path::QuorumEnrollment, QUORUM_ENROLLMENT),
    (Path::RecoveryCode, RECOVERY_CODE),
    (Path::ContactShares, CONTACT_SHARES),
    (Path::PasskeyWrap, PASSKEY_WRAP),
    (Path::EscrowRecovery, ESCROW_RECOVERY),
    (Path::EscrowOpen, ESCROW_OPEN),
];
