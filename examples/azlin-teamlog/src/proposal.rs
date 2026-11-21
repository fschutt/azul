//! The entry as it arrives and the witness that its signatures hold (planning/architecture.md section
//! 5.9 "The team log and the space chains": the fields `v`, `account`, `seq`, `prev`, `at`, `signer`,
//! `op`/`args`, `sealed`, `cosigs`, `webauthn`, `sig`; build unit S17, lane W1-S17c2).
//!
//! [`RawEntry`] is the entry decoded by the glue (azlin-cbor, S5's contract types): the typed fields,
//! the `preimage` (the byte string every signature of the entry covers: `domain || 0x00 || canonical
//! bytes`, [`TEAM_LOG_DOMAIN`]) and its `hash` ([`crate::hash::entry_hash`] of the preimage). The typed
//! fields are what `admit` reads, the preimage is what the signatures cover, and nothing in a `RawEntry`
//! ties the two: `verify_signatures` does, through the [`EntryDecoder`](crate::decode::EntryDecoder)
//! seam (it decodes the preimage itself and refuses an entry any of whose typed fields differs from the
//! signed bytes: X-DELTA-W1S47A-1). A signature over the preimage is a signature over the entry hash,
//! because `verify_signatures` first checks that the hash is the hash of the preimage.
//!
//! [`Proposal`] is a witness: its fields are private, it has no `Clone`, `Default` or `From`, and the
//! only function that makes one is `verify_signatures`. `admit` takes a `Proposal`, never a `RawEntry`,
//! so an entry whose signatures were not checked, or whose typed fields are not the signed ones, cannot
//! be admitted.

use azlin_seams::{
    AccountId, ByteCount, ChainSeq, DeviceId, Domain, Hash32, KeyId, RestoreEpoch, Stamp,
};

use crate::hash::entry_hash;
use crate::ops::{Op, OpName};

/// The signature domain of a team-log entry (planning/architecture.md section 2.5: "azlin-team-log-v1").
/// The preimage of every signature of an entry is `signing_input(TEAM_LOG_DOMAIN, canonical bytes)`.
///
/// Implements planning/architecture.md section 2.5 (signed structures, each with its own domain
/// string) and COMMON-W1R R4 (one signing rule).
///
/// Caller: the glue that builds and checks the preimage (CHAIN-GLUE), the tests.
pub const TEAM_LOG_DOMAIN: Domain = Domain::new("azlin-team-log-v1");

/// The most cosignatures an entry may carry (est.): a quorum of at most `MAX_ADMIN_QUORUM` (8) people,
/// each possibly with a second device.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up front).
///
/// Caller: `verify::verify_signatures`.
pub const COSIGS_MAX: usize = 16;

/// The most passkey assertions an entry may carry (est.): one for each signer of a quorum.
///
/// Implements planning/architecture.md section 1.4 rule 4.
///
/// Caller: `verify::verify_signatures`.
pub const ASSERTIONS_MAX: usize = 16;

/// The longest signature an entry may carry, in bytes: Ed25519 is 64, ML-DSA-65 is 3,309, and the
/// bound leaves room for neither to grow (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.9 (`sig`, `pq_sig`).
///
/// Caller: `verify::verify_signatures`.
pub const SIG_MAX_LEN: usize = 4096;

/// The largest preimage of an entry: "a few hundred bytes each in CBOR" (planning/architecture.md
/// section 5.9), and an app op carries up to `APP_BODY_MAX`; the bound is a few times that (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4.
///
/// Caller: `verify::verify_signatures`.
pub const PREIMAGE_MAX: ByteCount = ByteCount::kib(64);

/// Who signed an entry (planning/architecture.md section 5.9 `signer`; drift row X-DELTA-W1S17C-3: the
/// signer is a key, a device id is the key id of a device key): a device, a delegate key (the identity
/// service, the key agent), or the payments service's key (an `entitlement`).
///
/// A delegate key is identified by the bytes of its Ed25519 public key, like the fake keys of
/// `azlin_seams::FakeScheme`: `KeyId::as_bytes` of a delegate key IS the key that verifies it.
///
/// Implements planning/architecture.md section 5.9 (`signer`, `delegate`, `entitlement`).
///
/// Caller: `RawEntry::signer`, `verify::verify_signatures`, `admit.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Signer {
    /// A device of a member (or, for a `genesis` and a `member_join`, the new device itself).
    Device(DeviceId),
    /// A delegate key named by a `delegate` entry.
    DelegateKey(KeyId),
    /// The payments service's key.
    PaymentsKey,
}

/// What `azlin-webauthn` (lane S17d) says about one passkey assertion over an entry's hash: whether the
/// user was verified, whether the assertion's challenge is THIS entry's hash, whether the relying party
/// is ours, and which device's key made it. Defined here as a plain struct until S17d lands (brief,
/// section 8); the glue maps S17d's type to it.
///
/// Implements planning/architecture.md section 5.9 (`webauthn`: "a passkey assertion with user
/// verification over the entry hash") and section 7.5 (`escrow_open`: "each with a passkey assertion
/// with user verification").
///
/// Caller: `RawEntry::assertions`, `admit.rs` (`escrow_open`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AssertionVerdict {
    /// The device whose key made the assertion.
    pub key: DeviceId,
    /// Whether the user was verified (a PIN, a biometric), not only present.
    pub user_verified: bool,
    /// Whether the assertion's challenge is the hash of this entry.
    pub challenge_matches: bool,
    /// Whether the assertion is for our relying party.
    pub rp_ok: bool,
}

impl AssertionVerdict {
    /// Whether the assertion counts: user verified, over this entry's hash, for our relying party.
    ///
    /// Implements planning/architecture.md section 7.5 (a passkey assertion with user verification).
    ///
    /// Caller: `admit.rs` (`escrow_open`).
    pub const fn holds(&self) -> bool {
        self.user_verified && self.challenge_matches && self.rp_ok
    }
}

/// One entry of the team log as it arrives (planning/architecture.md section 5.9, the table of
/// fields). Every field is plain data: nothing in it has been checked. The account, the position, `at`,
/// the restore epoch, the signer and the op are SIGNED fields: `verify_signatures` checks each against
/// the decode of `preimage`. `sig`, `cosigs` and `assertions` are not part of the signed bytes (they sign
/// or vouch for them); `sealed` is bound to them by its authentication tag instead (its associated
/// data is the entry hash), so it is outside the preimage.
///
/// `at` is informational (the authority table v0: time is the leader's stamp, section 2.4); `epoch` is
/// the restore epoch the entry was written in (drift row X-DELTA-W1S17A-3); `pq_sig` is not carried:
/// nothing verifies it yet (`Proposal::pq_ok` is `None`).
///
/// Implements planning/architecture.md section 5.9.
///
/// Caller: the glue that decodes a wire entry (CHAIN-GLUE), `verify::verify_signatures`, `replay`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RawEntry {
    /// The account the entry belongs to.
    pub account: AccountId,
    /// The position the entry claims.
    pub seq: ChainSeq,
    /// The hash of the entry before it; zero for a genesis.
    pub prev: Hash32,
    /// The hash of the entry: `entry_hash` of the preimage.
    pub hash: Hash32,
    /// The time the signer's device claims (informational).
    pub at: Stamp,
    /// The restore epoch the entry was written in.
    pub epoch: RestoreEpoch,
    /// Who signed.
    pub signer: Signer,
    /// The op and its arguments.
    pub op: Op,
    /// The signer's signature over the preimage.
    pub sig: Vec<u8>,
    /// Further signatures over the same preimage, by devices (a quorum, a confirmation).
    pub cosigs: Vec<(DeviceId, Vec<u8>)>,
    /// The passkey assertions, already checked by `azlin-webauthn`.
    pub assertions: Vec<AssertionVerdict>,
    /// The bytes every signature covers.
    pub preimage: Vec<u8>,
    /// Display data (names, emails) encrypted under the account's seal key.
    pub sealed: Option<Vec<u8>>,
}

impl RawEntry {
    /// The name of the entry's op.
    ///
    /// Implements planning/architecture.md section 5.9 (the table of ops).
    ///
    /// Caller: the glue (the wire op word), the tests.
    pub fn op_name(&self) -> OpName {
        self.op.name()
    }

    /// Whether this copy is intact: its hash is the hash of its preimage. A copy that is not is
    /// fetched again from another voter (planning/architecture.md section 5.9).
    ///
    /// Implements planning/architecture.md section 5.9 ("an entry that fails its hash or signature
    /// check is fetched again from another voter").
    ///
    /// Caller: `verify::verify_signatures`, the glue's `azlin_chain::ChainEntry::is_intact`.
    pub fn is_intact(&self) -> bool {
        entry_hash(&self.preimage) == self.hash
    }
}

/// The keys the check trusts from outside the log: the payments service's public key or keys (an
/// `entitlement` is signed by it, planning/architecture.md section 5.9). The first is the current key;
/// the others are retired keys whose entitlements stay valid (section 2.4: "entitlements signed by the
/// old key stay valid until they expire").
///
/// Implements planning/architecture.md section 5.9 (`entitlement`: "replayed from the payments
/// service's submission") and X-D-11 (the key window).
///
/// Caller: `verify::verify_signatures`; the registry glue holds the keys of the map anchors.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TrustedKeys {
    payments: Vec<[u8; 32]>,
}

impl TrustedKeys {
    /// Trusts these payments keys.
    pub fn new(payments: Vec<[u8; 32]>) -> TrustedKeys {
        TrustedKeys { payments }
    }

    /// Trusts no payments key: no entitlement is admitted.
    pub fn none() -> TrustedKeys {
        TrustedKeys {
            payments: Vec::new(),
        }
    }

    /// The payments keys, the current one first.
    pub fn payments(&self) -> &[[u8; 32]] {
        &self.payments
    }
}

/// Who signed, as `verify_signatures` checked it.
#[derive(Debug)]
pub(crate) enum VerifiedSigner {
    /// A device whose key verified the signature.
    Device {
        /// The device.
        device: DeviceId,
        /// The key that verified.
        key: [u8; 32],
    },
    /// A delegate key.
    Delegate(KeyId),
    /// The payments key.
    Payments,
}

/// An entry whose signatures hold: the witness `admit` takes. Only `verify_signatures` makes one.
///
/// It records which signature verified (the signer's, and the cosignatures that did: a cosignature
/// that does not verify, or of an unknown device, is left out and counts for nothing) and keeps the
/// entry. `pq_ok` is `None` while nothing can check an ML-DSA signature (drift row X-DELTA-W1S17A-7:
/// a policy that demands one fails closed, but no policy does yet).
///
/// Implements planning/architecture.md section 5.9 ("every entry is signed by a device whose role
/// allows it") and the brief's `Proposal` witness.
///
/// Caller: `verify::verify_signatures` (makes it), `admit::admit` (consumes it by reference),
/// `replay::replay`.
#[derive(Debug)]
pub struct Proposal {
    entry: RawEntry,
    signer: VerifiedSigner,
    cosigners: Vec<DeviceId>,
    pq_ok: Option<bool>,
}

impl Proposal {
    /// Makes the witness: only `verify_signatures` calls it.
    pub(crate) fn new(
        entry: RawEntry,
        signer: VerifiedSigner,
        cosigners: Vec<DeviceId>,
    ) -> Proposal {
        Proposal {
            entry,
            signer,
            cosigners,
            pq_ok: None,
        }
    }

    /// The entry.
    pub fn entry(&self) -> &RawEntry {
        &self.entry
    }

    /// Whether the post-quantum signature verified: `None` while nothing can check it.
    pub fn pq_ok(&self) -> Option<bool> {
        self.pq_ok
    }

    /// The signer as it was verified.
    pub(crate) fn signer(&self) -> &VerifiedSigner {
        &self.signer
    }

    /// The devices whose cosignature verified over this entry's preimage, in the order of the entry,
    /// each once.
    pub(crate) fn cosigners(&self) -> &[DeviceId] {
        &self.cosigners
    }
}
