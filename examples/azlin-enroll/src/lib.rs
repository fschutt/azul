//! Azlin enroll: the state machine that decides when a key may be released to a new device
//! (planning/architecture.md section 7.5 "Enrollment and recovery that an attacker cannot use",
//! section 7.6 "Escrow and the key agent", section 5.5 "Recovery", section 5.9, section 12 E2;
//! build unit S17, lane W1-S17e).
//!
//! "No key is released before its hold ends on any path; an existing device sees every request
//! and can veto it" (E2). One request type, every path of 7.5 and 5.5 as data ([`PATHS`]), the hold
//! from the policy, cancellation, the escrow-opening rules (two different people, the key agent
//! never alone), the notices, the per-person limits and the fingerprint phrase. The metadata group
//! (S9) asks [`release()`], then [`mark_released()`] with the facts of the moment, and serves a
//! `member_key` wrap to a new device only with the [`Served`] that the second call returns; lane
//! S47b tests the matrix against this crate.
//!
//! | item | what | module |
//! |---|---|---|
//! | `Path`, `PATHS`, `PathRule` | the seven paths of 7.5 and 5.5 as data: hold, evidence, openers, gate, wrap | [`path`](mod@path) |
//! | `Request`, `open`, `PersonFacts`, `pending_for_person` | one attempt, its state machine, the facts it takes | [`request`](mod@request) |
//! | `Evidence`, `add_evidence` | what each path needs: SSO (subject checked), phrase, shares, passkey | [`evidence`](mod@evidence) |
//! | `HoldKind`, `hold_for` | the hold, fixed when the request is opened | [`hold`](mod@hold) |
//! | `cancel`, `Canceller` | the veto: any admin or active device, never helpdesk or a pending device | [`cancel`](mod@cancel) |
//! | `Contribution`, `contribute`, `shortfall` | escrow openings: two people, the key agent a share only | [`escrow_open`](mod@escrow_open) |
//! | `release`, `mark_released`, `ReleaseGrant`, `Served`, `StampCheck` | the decision, the standing of the ones it depends on, and the two witnesses | [`release`](mod@release) |
//! | `OpenLimits` | one pending request and three openings a day, per person and kind of opener | [`limits`](mod@limits) |
//! | `Notices`, `Notice`, `Audience` | the notices `log_appended` to every admin and every device | [`notices`](mod@notices) |
//! | `phrase_of`, `confirm_phrase`, `confirm_join` | the fingerprint phrase of 66 bits, the inviter's confirmation | [`phrase`](mod@phrase) |
//!
//! WHAT THE CRATE DOES NOT DO. It verifies no signature, token or assertion (the evidence are
//! verified facts that S17d, S17f, S17g and the glue make), does no cryptography, reads no clock
//! (every call takes the leader's `Stamp`; `open`, `add_evidence`, `release`, `mark_released` and
//! `expire` also the `StampCheck` of it from S40), keeps no storage (the group's state holds the
//! requests and the ledger) and sends nothing (the calls RETURN the notices to emit). It depends
//! on no team-log type: the facts are plain data that the glue builds (COMMON-W1R R5).
//!
//! A person with an active device approves a new one from it, with the fingerprint phrase and no
//! hold; everything else waits the hold, and an existing device or an admin can cancel:
//!
//! ```
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! use azlin_enroll::{
//!     add_evidence, cancel, mark_released, open, release, Canceller, Confirmer, Evidence,
//!     OpenLimits, Opener, Path, PersonFacts, Release, StampCheck, WrapKind,
//! };
//! use azlin_policy::{AccountKind, Policy};
//! use azlin_seams::{DeviceId, MemberId, Span, UnixTime};
//!
//! let alice = MemberId::from_bytes([1; 16]);
//! let laptop = DeviceId::from_bytes([1; 16]);
//! let phone = DeviceId::from_bytes([2; 16]);
//! let facts = PersonFacts {
//!     person: alice,
//!     managed: false,
//!     active_devices: vec![laptop],
//!     admins: vec![alice],
//!     admin_count: 1,
//!     escrow_configured: false,
//!     offboarded_by: 0,
//! };
//! let policy = Policy::defaults(AccountKind::Personal);
//! let mut limits = OpenLimits::standard();
//! let now = UnixTime::from_secs(1_800_000_000);
//! let agreed = StampCheck::Agreed;
//!
//! // The laptop approves the phone: the phrases matched, so there is no hold.
//! let opener = Opener::person_device(laptop, phone);
//! let (mut approval, _notices) = open(&facts, &policy, Path::OwnDevice, opener, now, agreed, &mut limits)?;
//! add_evidence(&mut approval, Evidence::PhraseConfirmed(Confirmer::Device(laptop)), now, agreed)?;
//! let grant = match release(&approval, now, agreed) {
//!     Release::Granted(grant) => grant,
//!     other => return Err(format!("not released: {other:?}").into()),
//! };
//! // The wrap goes out only with the `Served` of `mark_released`, which judges again with the
//! // facts of the moment who still stands (the laptop that approved, here).
//! let served = mark_released(&mut approval, &facts, grant, now, agreed)?;
//! assert_eq!(served.new_device(), phone);
//! assert_eq!(served.wrap(), WrapKind::SealedToDevice);
//!
//! // A recovery code opens a request that waits 48 hours, whatever else is in order.
//! let tablet = DeviceId::from_bytes([3; 16]);
//! let later = now.checked_add(Span::hours(1)).ok_or("time")?;
//! let (mut recovery, notices) = open(
//!     &facts, &policy, Path::RecoveryCode, Opener::person(tablet), later, agreed, &mut OpenLimits::standard(),
//! )?;
//! add_evidence(&mut recovery, Evidence::RecoveryCodeProof { device: tablet }, later, agreed)?;
//! assert!(!release(&recovery, later, agreed).is_granted());
//! assert_eq!(notices.devices(), vec![laptop]); // the laptop is told, and can veto it
//! cancel(&mut recovery, &facts, Canceller::PersonDevice(laptop), later)?;
//! let after_the_hold = later.checked_add(Span::hours(48)).ok_or("time")?;
//! assert!(!release(&recovery, after_the_hold, agreed).is_granted());
//! # Ok(())
//! # }
//! ```

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

pub mod cancel;
pub mod escrow_open;
pub mod evidence;
pub mod hold;
pub mod limits;
pub mod notices;
pub mod path;
pub mod phrase;
pub mod release;
pub mod request;

pub use cancel::{cancel, CancelRefusal, Canceller};
pub use escrow_open::{
    contribute, shortfall, shortfall_for, Contribution, ContributionKind, ContributionRefusal,
    KeyAgentCheck, KeyAgentChecks, Shortfall, ESCROW_MIN_SHARES, ESCROW_PEOPLE, MAX_CONTRIBUTIONS,
};
pub use evidence::{
    add_evidence, Confirmer, ContactSlot, DeviceEvidenceKind, Evidence, EvidenceKind,
    EvidenceRefusal, SlotOutOfRange, CONTACT_SHARES_NEEDED, CONTACT_SHARES_TOTAL, MAX_EVIDENCE,
    SSO_FRESH,
};
pub use hold::{
    hold_for, hold_length, life_after, HoldKind, EXPIRES_AFTER_HOLD, OFFBOARDING_QUORUM_MIN,
    OWN_DEVICE_LIFE,
};
pub use limits::{
    LimitRefusal, LimitsInvalid, OpenLimits, DAILY_WINDOW, MAX_LEDGERS, STANDARD_PENDING_MAX,
    STANDARD_PER_DAY,
};
pub use notices::{Audience, Notice, NoticeKind, Notices};
pub use path::{OpenerKind, Path, PathRule, PolicyGate, WrapKind, PATHS};
pub use phrase::{
    confirm_join, confirm_phrase, phrase_of, DeviceKeys, JoinConfirmation, JoinRefusal, Phrase,
    PhraseMismatch, Transcript, Wordlist, WordlistGap, PHRASE_CONTEXT, PHRASE_WORDS, WORDLIST_LEN,
    WORD_BITS,
};
pub use release::{
    expire, mark_released, release, Reason, Release, ReleaseGrant, ReleaseRefusal, Served,
    StampCheck, StandingLost,
};
pub use request::{
    open, pending, pending_for_person, FactsRefusal, OpenRefusal, OpenedBy, Opener, PersonFacts,
    Request, RequestId, RequestState, MAX_LIST, MAX_SIGNERS, REQUEST_CONTEXT,
};

/// The ids of the mutation guards of this crate (`azlin_seams::mutant!`), each with the test that
/// must fail when the guard is on: the hold that proves the tests can fail runs
/// `AZLIN_MUTANT=<id> cargo test -p azlin-enroll --features azlin-seams/mutants` once for each id
/// (it must fail) and once without the variable (it must pass). A guard is listed here when it is in
/// the source (`tests/hygiene.rs` reads both). The six are the guards that lane S47b must catch.
///
/// | id | what the guard turns off | killer |
/// |---|---|---|
/// | `enroll_hold_skipped_on_recovery` | the hold of the recovery paths (`hold_for`) | `hold_withholds_release_before_hold_until_recovery_code` and its five siblings |
/// | `enroll_cancel_by_helpdesk` | "helpdesk cannot cancel" | `helpdesk_cannot_cancel_or_approve_alone` |
/// | `enroll_sso_subject_unchecked` | "the approver checks the SSO attestation's subject" | `sso_subject_must_match_the_person` |
/// | `enroll_agent_counts_as_person` | "the key agent's share never stands in for a person" | `key_agent_share_never_stands_in_for_a_person` |
/// | `enroll_disputed_stamp_released` | "a disputed stamp never releases" | `disputed_stamp_never_releases` |
/// | `enroll_pending_device_may_cancel` | "a pending device cannot cancel" | `a_pending_device_cannot_cancel_the_request_it_belongs_to_or_anyone_elses` |
///
/// Implements planning/architecture.md section 12 (E2) and section 1.4 rule 1 (the guard reads its
/// switch as a parameter of the process).
///
/// Seam: the mutant hold of lane W1-S17e, lane S47b (`azlin-escalate-e2` catches the same six).
pub const MUTANT_IDS: [&str; 6] = [
    "enroll_hold_skipped_on_recovery",
    "enroll_cancel_by_helpdesk",
    "enroll_sso_subject_unchecked",
    "enroll_agent_counts_as_person",
    "enroll_disputed_stamp_released",
    "enroll_pending_device_may_cancel",
];
