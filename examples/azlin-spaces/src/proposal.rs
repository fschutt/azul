//! The space-chain entry as it arrives, and the witness that its signatures hold (planning/architecture.md
//! section 5.9 "Space chains": every entry is signed by a device whose member may sign it; section 2.5
//! "Signed structures, each with its own domain string: azlin-space-chain-v1"; build unit S16, lane
//! W1-S16b).
//!
//! [`SpaceEntry`] is the entry decoded by the glue (azlin-cbor, S5's contract types): the typed fields,
//! the `preimage` (the byte string every signature of the entry covers: `signing_input(`
//! [`SPACE_CHAIN_DOMAIN`]`, canonical bytes)`) and its `hash` ([`entry_hash`] of the preimage). The glue
//! decodes strictly and keeps the signed bytes as the preimage, so the typed fields ARE the signed
//! content: this crate never parses a preimage and never builds one.
//!
//! [`SpaceProposal`] is a witness: its fields are private, it has no `Clone`, `Default` or `From`, and
//! the only function that makes one is [`verify_entry`]. `admit` takes a `SpaceProposal`, never a
//! `SpaceEntry`, so an entry whose signatures were not checked cannot be admitted.
//!
//! ```compile_fail,E0451
//! // A proposal cannot be built by hand: its fields are private.
//! use azlin_spaces::SpaceProposal;
//! let _forged = SpaceProposal { entry: todo!(), cosigners: vec![] };
//! ```

use azlin_chain::ChainRefusal;
use azlin_keys::DirectoryError;
use azlin_seams::{
    hash32, mutant, ByteCount, ChainSeq, DeviceDirectory, DeviceId, Domain, Hash32, RestoreEpoch,
    SigAlg, SignatureVerifier, SpaceId, Stamp, VerifyAt,
};

use crate::admit::SpaceRefusal;
use crate::ops::SpaceOp;

/// The signature domain of a space-chain entry (planning/architecture.md section 2.5:
/// "azlin-space-chain-v1"). The preimage of every signature of an entry is
/// `signing_input(SPACE_CHAIN_DOMAIN, canonical bytes)`.
///
/// Implements planning/architecture.md section 2.5 (signed structures, each with its own domain string)
/// and COMMON-W1R R4 (one signing rule).
///
/// Caller: the glue that builds and checks the preimage (CHAIN-GLUE), the tests.
pub const SPACE_CHAIN_DOMAIN: Domain = Domain::new("azlin-space-chain-v1");

/// The hash domain of [`entry_hash`] (a BLAKE3 `derive_key` context, never a signature domain; drift row
/// X-DELTA-W1S16B-13).
///
/// Implements planning/architecture.md section 5.9 (hash-chained entries).
///
/// Caller: `entry_hash`.
pub const ENTRY_HASH_DOMAIN: &str = "azlin-spaces-entry-v1";

/// The most cosignatures an entry may carry (est.): a quorum of at most `MAX_ADMIN_QUORUM` (8) people,
/// each possibly with a second device.
///
/// Implements planning/architecture.md section 1.4 rule 4 (everything has a limit, stated up front).
///
/// Caller: `verify_entry`.
pub const COSIGS_MAX: usize = 16;

/// The longest signature an entry may carry, in bytes: Ed25519 is 64, ML-DSA-65 is 3,309, and the bound
/// leaves room for neither to grow (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.9 (`sig`, `pq_sig`).
///
/// Caller: `verify_entry`.
pub const SIG_MAX_LEN: usize = 4096;

/// The largest preimage of an entry: a space-chain entry is "a few hundred bytes each in CBOR" (section
/// 5.9); the bound is a few dozen times that (est.).
///
/// Implements planning/architecture.md section 1.4 rule 4 and section 5.9.
///
/// Caller: `verify_entry`.
pub const PREIMAGE_MAX: ByteCount = ByteCount::kib(16);

/// The hash of an entry: BLAKE3 under [`ENTRY_HASH_DOMAIN`] of its preimage. A copy whose `hash` is
/// not this is corrupt and is fetched again from another voter.
///
/// Implements planning/architecture.md section 5.9 ("an entry that fails its hash or signature check is
/// fetched again from another voter").
///
/// Caller: `verify_entry`, `SpaceEntry::is_intact`, the glue that builds an entry (CHAIN-GLUE).
pub fn entry_hash(preimage: &[u8]) -> Hash32 {
    hash32(ENTRY_HASH_DOMAIN, &[preimage])
}

/// A cosignature: a further device's signature over the same preimage (the admin quorum of an override).
///
/// Implements planning/architecture.md section 5.9 (`cosigs`: "further admin signatures for a quorum").
///
/// Caller: `SpaceEntry::cosigs`, `verify_entry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cosig {
    /// The cosigning device.
    pub device: DeviceId,
    /// Its signature over the entry's preimage.
    pub sig: Vec<u8>,
}

/// One entry of a space chain as it arrives. Every field is plain data: nothing in it has been checked.
///
/// Implements planning/architecture.md section 5.9 (the chain fields `seq`, `prev`, `signer`, `op`,
/// `cosigs`, `sig`) and section 6.10 V3 (every entry names its restore epoch). The leader's stamp is not
/// a field: it is stored beside the entry and passed to `admit` (section 2.4).
///
/// Caller: the glue that decodes a wire entry (CHAIN-GLUE), `verify_entry`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SpaceEntry {
    /// The space the entry belongs to.
    pub space: SpaceId,
    /// The position the entry claims.
    pub seq: ChainSeq,
    /// The hash of the entry before it; zero for the `space_create`.
    pub prev: Hash32,
    /// The hash of the entry: `entry_hash` of the preimage.
    pub hash: Hash32,
    /// The restore epoch the entry was written in.
    pub epoch: RestoreEpoch,
    /// The device that signed.
    pub signer: DeviceId,
    /// The op and its arguments.
    pub op: SpaceOp,
    /// The signer's signature over the preimage.
    pub sig: Vec<u8>,
    /// Further signatures over the same preimage (the admin quorum).
    pub cosigs: Vec<Cosig>,
    /// The bytes every signature covers.
    pub preimage: Vec<u8>,
}

impl SpaceEntry {
    /// Whether this copy is intact: its hash is the hash of its preimage.
    pub fn is_intact(&self) -> bool {
        entry_hash(&self.preimage) == self.hash
    }
}

/// An entry whose signatures hold: the witness `admit` takes. Only [`verify_entry`] makes one.
///
/// It keeps the entry and which cosignatures verified (a cosignature that does not verify, or that is by
/// a device the directory does not know, is left out and counts for nothing toward a quorum).
///
/// Implements planning/architecture.md section 5.9 ("every entry is signed by a device whose member may
/// sign it") and the brief's `SpaceProposal`.
///
/// Caller: `verify_entry` (makes it), `admit` (takes it by reference), `replay`.
#[derive(Debug)]
pub struct SpaceProposal {
    entry: SpaceEntry,
    cosigners: Vec<DeviceId>,
}

impl SpaceProposal {
    /// The entry.
    pub fn entry(&self) -> &SpaceEntry {
        &self.entry
    }

    /// The devices whose cosignature verified over this entry's preimage, in the order of the entry,
    /// each once.
    pub(crate) fn cosigners(&self) -> &[DeviceId] {
        &self.cosigners
    }
}

/// The refusal of a part that is too big.
fn too_big(what: &'static str) -> SpaceRefusal {
    SpaceRefusal::TooBig(what)
}

/// The size of every part of the entry against its limit.
fn check_shape(e: &SpaceEntry) -> Result<(), SpaceRefusal> {
    if ByteCount::of_len(e.preimage.len()) > PREIMAGE_MAX {
        return Err(too_big("the preimage of an entry"));
    }
    if e.sig.len() > SIG_MAX_LEN {
        return Err(too_big("the signature of an entry"));
    }
    if e.cosigs.len() > COSIGS_MAX || e.cosigs.iter().any(|cosig| cosig.sig.len() > SIG_MAX_LEN) {
        return Err(too_big("the cosignatures of an entry"));
    }
    Ok(())
}

/// Why a device's key cannot be used for a signature.
fn key_refusal(why: DirectoryError) -> SpaceRefusal {
    match why {
        DirectoryError::Unknown => SpaceRefusal::SignerUnknown,
        DirectoryError::NotYetActive | DirectoryError::Revoked => SpaceRefusal::SignerNotActive,
        DirectoryError::Unavailable => SpaceRefusal::KeyUnavailable,
    }
}

/// Checks the signatures of an entry and makes the `SpaceProposal` that `admit` takes. `keys` is the
/// device directory (the key valid for a device at the stamp `at`), `v` the signature seam
/// (`azlin_seams::Ed25519Verifier` in production, `FakeScheme` in tests).
///
/// The order of the checks: the size of every part, the integrity of the copy (a copy that is not intact
/// is `Chain(Corrupt)`: fetch it again), the signer's key (unknown, not active, or unreadable are three
/// different refusals), the signer's signature (`BadSignature`), then each cosignature (the ones that
/// hold are kept, the others dropped).
///
/// Implements planning/architecture.md section 5.9 (`sig`, `cosigs`; "an entry that fails its hash or
/// signature check is fetched again from another voter") and section 2.5 (the domain).
///
/// Caller: the node's apply path and the device's chain check (through SPACES-APPLY and CHAIN-GLUE),
/// `replay`'s callers, the tests.
pub fn verify_entry(
    e: &SpaceEntry,
    keys: &dyn DeviceDirectory,
    v: &dyn SignatureVerifier,
    at: Stamp,
) -> Result<SpaceProposal, SpaceRefusal> {
    check_shape(e)?;
    if !e.is_intact() && !mutant!("spaces_hash_unchecked") {
        return Err(SpaceRefusal::Chain(ChainRefusal::Corrupt));
    }
    let key = keys
        .verify_key(&e.signer, VerifyAt::Time(at))
        .map_err(key_refusal)?;
    let signed = v
        .verify(SigAlg::Ed25519, key.as_bytes(), &e.preimage, &e.sig)
        .is_ok();
    if !signed && !mutant!("spaces_signature_unchecked") {
        return Err(SpaceRefusal::BadSignature);
    }
    let mut cosigners: Vec<DeviceId> = Vec::new();
    for cosig in &e.cosigs {
        if cosigners.contains(&cosig.device) {
            continue;
        }
        let Ok(cosigner_key) = keys.verify_key(&cosig.device, VerifyAt::Time(at)) else {
            continue;
        };
        let holds = mutant!("spaces_cosig_unchecked")
            || v.verify(
                SigAlg::Ed25519,
                cosigner_key.as_bytes(),
                &e.preimage,
                &cosig.sig,
            )
            .is_ok();
        if holds {
            cosigners.push(cosig.device);
        }
    }
    Ok(SpaceProposal {
        entry: e.clone(),
        cosigners,
    })
}
