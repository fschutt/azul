//! The seam between the typed entry and the bytes its signatures cover (planning/architecture.md section
//! 5.9 "every entry is signed by a device whose role allows it" and "The metadata group rejects an entry
//! with a reason for a wrong `prev`", section 2.5 "Signed structures, each with its own domain string",
//! section 12 E1 "no entry is accepted unless its signer may sign it"; build unit S17, lane W1-S17c3;
//! X-DELTA-W1S47A-1).
//!
//! WHY. `admit` reads typed fields of a [`RawEntry`](crate::proposal::RawEntry) (its position `seq` and
//! `prev`, its restore `epoch`, its `signer`, its `op`, its `account`); the signatures cover the PREIMAGE,
//! the canonical bytes. Until
//! this seam, nothing but a promise of the glue tied the two together, and the promise was not kept by
//! anything the crate could see: an old, validly signed entry whose typed position fields were changed to
//! those of a new entry was admitted, and `apply` would have made a member a helpdesk member again with no
//! fresh signature by anybody (X-DELTA-W1S47A-1). The same hole let any field of a `RawEntry` differ from
//! its preimage.
//!
//! THE BINDING IS INSIDE THE CRATE. `verify_signatures` takes an [`EntryDecoder`], decodes the signed bytes
//! ITSELF ([`canonical_of`] checks the signature domain first) and refuses an entry any of whose typed
//! fields differs from what the bytes say, with `Refusal::BadSignature` (class Signature: the signatures
//! do not cover this entry). A [`Proposal`](crate::proposal::Proposal) therefore means: the signatures hold
//! AND the typed fields are the signed fields. The decoder is the glue's: it parses the canonical bytes
//! strictly with the engine's codec (azlin-cbor, S5's contract types); this crate never parses a
//! preimage and never builds one.
//!
//! WHAT IS SIGNED: the account, the position (`seq`, `prev`), the signer's own claim `at` (informational:
//! the leader's stamp decides), the restore `epoch`, the `signer` and the `op` with its arguments. WHAT IS
//! NOT: `sig` and `cosigs` (they sign the
//! preimage), `assertions` (the verdicts of `azlin-webauthn` over the entry hash) and `sealed` (display data
//! encrypted under the account's seal key with the entry's hash as associated data: it is bound to the
//! signed bytes by its authentication tag, and cannot be inside the bytes whose hash it is bound to).
//!
//! [`FakeDecoder`] is the TEST DOUBLE of the seam, like `azlin_seams::FakeScheme` for signatures: it decodes
//! the canonical bytes that were registered with it, and nothing else. Production never uses it.

use std::cell::RefCell;
use std::collections::BTreeMap;

use azlin_seams::{AccountId, ChainSeq, Hash32, RestoreEpoch, Stamp};

use crate::ops::Op;
use crate::proposal::{Signer, TEAM_LOG_DOMAIN};

/// The fields of an entry that its signatures cover, as the signed bytes say them.
///
/// Implements planning/architecture.md section 5.9 (the entry's fields: `account`, `seq`, `prev`, `at`,
/// `signer`, `op`/`args`; the restore epoch of X-DELTA-W1S17A-3) and X-DELTA-W1S47A-1.
///
/// Caller: `EntryDecoder::decode`, `verify::verify_signatures` (compares each field with the typed
/// entry), the glue's decoder (CHAIN-GLUE), `FakeDecoder`, the tests.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DecodedEntry {
    /// The account the entry belongs to.
    pub account: AccountId,
    /// The position the entry claims.
    pub seq: ChainSeq,
    /// The hash of the entry before it; zero for a genesis.
    pub prev: Hash32,
    /// The time the signer's device claims (informational: the leader's stamp decides).
    pub at: Stamp,
    /// The restore epoch the entry was written in.
    pub epoch: RestoreEpoch,
    /// Who signed.
    pub signer: Signer,
    /// The op and its arguments.
    pub op: Op,
}

/// The signed bytes are not a team-log entry (malformed, trailing bytes, an unknown version, an unknown op
/// word): the one thing a decoder says when it cannot decode.
///
/// Implements planning/architecture.md section 5.9 ("an entry that fails its hash or signature check is
/// fetched again from another voter").
///
/// Caller: `EntryDecoder::decode`, `verify::verify_signatures` (maps it to `Refusal::BadSignature`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
#[error("the signed bytes are not a team-log entry")]
pub struct NotAnEntry;

/// The decoder of the bytes an entry's signatures cover: the seam that lets `verify_signatures` bind the
/// typed fields to the signed bytes without parsing anything itself.
///
/// `decode` takes the CANONICAL bytes: what follows the signature domain and its separator in the
/// preimage (`signing_input(TEAM_LOG_DOMAIN, canonical)`). It must decode strictly: a byte it does not
/// understand, a trailing byte, a field out of its range is `NotAnEntry`, never a default.
///
/// Implements planning/architecture.md section 5.9 (the fields of an entry) and section 1.4 rule 2
/// (codecs are pure and strict).
///
/// Caller: `verify::verify_signatures`, `replay::replay`; implemented by the glue over S5's contract types
/// (CHAIN-GLUE) and by `FakeDecoder` in tests (S47a, SIM, this crate's tests).
pub trait EntryDecoder {
    /// The entry the canonical bytes encode, or `NotAnEntry`.
    fn decode(&self, canonical: &[u8]) -> Result<DecodedEntry, NotAnEntry>;
}

/// The canonical bytes of a preimage: what follows the signature domain `TEAM_LOG_DOMAIN` and its NUL
/// separator. `None` when the preimage does not begin with them: a signature over bytes of another domain
/// is no signature of a team-log entry (planning/architecture.md section 2.5: each structure has its own
/// domain string).
///
/// Implements planning/architecture.md section 2.5.
///
/// Caller: `verify::verify_signatures`, the tests.
pub fn canonical_of(preimage: &[u8]) -> Option<&[u8]> {
    preimage
        .strip_prefix(TEAM_LOG_DOMAIN.as_str().as_bytes())?
        .strip_prefix(&[0u8])
}

/// A decoder over a table, for tests: canonical bytes registered with the entry they stand for decode to
/// it, and nothing else decodes. TEST DOUBLE of the seam (like `azlin_seams::FakeScheme`): production uses
/// the strict decoder of the wire codec, never this.
///
/// Implements planning/architecture.md section 1.4 rule 1 (every seam has a test double).
///
/// Caller: the tests of this crate (`SignedKit`), the escalation lane S47a (its rig registers every entry
/// it converts), SIM.
#[derive(Clone, Debug, Default)]
pub struct FakeDecoder {
    table: RefCell<BTreeMap<Vec<u8>, DecodedEntry>>,
}

impl FakeDecoder {
    /// A decoder that knows nothing yet.
    ///
    /// Implements planning/architecture.md section 1.4 rule 1.
    ///
    /// Caller: the tests.
    pub fn new() -> FakeDecoder {
        FakeDecoder::default()
    }

    /// Registers what `canonical` stands for; returns what it stood for before, if anything (a test that
    /// gives two entries the same bytes sees the first one come back).
    ///
    /// Implements planning/architecture.md section 1.4 rule 1.
    ///
    /// Caller: the tests, S47a's rig.
    pub fn register(&self, canonical: Vec<u8>, entry: DecodedEntry) -> Option<DecodedEntry> {
        self.table.borrow_mut().insert(canonical, entry)
    }

    /// How many byte strings are registered.
    ///
    /// Implements planning/architecture.md section 1.4 rule 1.
    ///
    /// Caller: the tests.
    pub fn len(&self) -> usize {
        self.table.borrow().len()
    }

    /// Whether nothing is registered.
    ///
    /// Implements planning/architecture.md section 1.4 rule 1.
    ///
    /// Caller: the tests.
    pub fn is_empty(&self) -> bool {
        self.table.borrow().is_empty()
    }
}

impl EntryDecoder for FakeDecoder {
    fn decode(&self, canonical: &[u8]) -> Result<DecodedEntry, NotAnEntry> {
        self.table
            .borrow()
            .get(canonical)
            .cloned()
            .ok_or(NotAnEntry)
    }
}
