//! Helpers the test files share: a small chain entry that hashes its fields with BLAKE3, seeded
//! chains, and fake signers and verifiers of heads and restore records over `FakeScheme`.
//!
//! The signed bytes of a head and of a restore record are the TEST's own (a fixed layout under a test
//! domain, the restore epoch included: it is part of what a head signature covers): the real ones
//! are the glue's (azlin-cbor, `signing_input`), and the crate under test never builds them.
#![allow(dead_code)]

use azlin_chain::{
    ChainEntry, ChainId, Head, HeadVerifier, RestoreRecord, RestoreSig, RestoreSigVerifier,
    RestoreSigner, SignedHead,
};
use azlin_seams::sim::Rng;
use azlin_seams::{
    hash32, signing_input, AccountId, ChainSeq, Domain, FakeScheme, Hash32, KeyId, RestoreEpoch,
    SigAlg, SignatureVerifier, SpaceId, Stamp, UnixTime, VerifyError,
};

pub const ENTRY_DOMAIN: &str = "azlin-chain-test-entry-v1";
pub const HEAD_DOMAIN: Domain = Domain::new("azlin-chain-test-head-v1");
pub const RESTORE_DOMAIN: Domain = Domain::new("azlin-chain-test-restore-v1");

pub fn at(secs: u64) -> Stamp {
    UnixTime::from_secs(secs)
}

pub fn seq(n: u64) -> ChainSeq {
    ChainSeq::new(n)
}

pub fn epoch(n: u64) -> RestoreEpoch {
    RestoreEpoch::new(n)
}

pub fn team_log(fill: u8) -> ChainId {
    ChainId::team_log(AccountId::from_bytes([fill; 16]))
}

pub fn space(fill: u8) -> ChainId {
    ChainId::space(SpaceId::from_bytes([fill; 16]))
}

pub fn key(fill: u8) -> KeyId {
    KeyId::from_bytes([fill; 32])
}

/// A hash that is nobody's: `fill` repeated.
pub fn hash_of_fill(fill: u8) -> Hash32 {
    Hash32::from_bytes([fill; 32])
}

// ---------------------------------------------------------------------------------------------
// The test entry
// ---------------------------------------------------------------------------------------------

/// A chain entry of the tests: its hash is BLAKE3 over `(chain, seq, prev, at, epoch, payload)`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TestEntry {
    pub chain: ChainId,
    pub seq: ChainSeq,
    pub prev: Hash32,
    pub at: Stamp,
    pub epoch: RestoreEpoch,
    pub payload: Vec<u8>,
    pub hash: Hash32,
}

impl TestEntry {
    pub fn hash_of(
        chain: &ChainId,
        seq: ChainSeq,
        prev: &Hash32,
        at: Stamp,
        epoch: RestoreEpoch,
        payload: &[u8],
    ) -> Hash32 {
        let kind = [chain.kind as u8];
        let seq_bytes = seq.get().to_be_bytes();
        let at_bytes = at.as_secs().to_be_bytes();
        let epoch_bytes = epoch.get().to_be_bytes();
        let parts: [&[u8]; 7] = [
            &kind,
            &chain.id,
            &seq_bytes,
            prev.as_bytes(),
            &at_bytes,
            &epoch_bytes,
            payload,
        ];
        hash32(ENTRY_DOMAIN, &parts)
    }

    /// An intact entry of the restore epoch 0: its hash is computed from its fields.
    pub fn new(
        chain: ChainId,
        seq: ChainSeq,
        prev: Hash32,
        at: Stamp,
        payload: &[u8],
    ) -> TestEntry {
        TestEntry::new_in(chain, seq, prev, at, RestoreEpoch::ZERO, payload)
    }

    /// An intact entry of the given restore epoch.
    pub fn new_in(
        chain: ChainId,
        seq: ChainSeq,
        prev: Hash32,
        at: Stamp,
        epoch: RestoreEpoch,
        payload: &[u8],
    ) -> TestEntry {
        let hash = TestEntry::hash_of(&chain, seq, &prev, at, epoch, payload);
        TestEntry {
            chain,
            seq,
            prev,
            at,
            epoch,
            payload: payload.to_vec(),
            hash,
        }
    }

    pub fn genesis(chain: ChainId, at: Stamp, payload: &[u8]) -> TestEntry {
        TestEntry::new(chain, seq(0), Hash32::ZERO, at, payload)
    }

    /// The entry after this one, in the same restore epoch.
    pub fn after(&self, at: Stamp, payload: &[u8]) -> TestEntry {
        self.after_in(at, self.epoch, payload)
    }

    /// The entry after this one, in the given restore epoch.
    pub fn after_in(&self, at: Stamp, epoch: RestoreEpoch, payload: &[u8]) -> TestEntry {
        let next = self.seq.next().expect("a test chain is short");
        TestEntry::new_in(self.chain, next, self.hash, at, epoch, payload)
    }

    /// A copy whose content changed but whose claimed hash did not: a corrupt copy.
    pub fn corrupted(&self) -> TestEntry {
        let mut copy = self.clone();
        copy.payload.push(0xEE);
        copy
    }

    pub fn head(&self) -> Head {
        Head {
            chain: self.chain,
            seq: self.seq,
            hash: self.hash,
        }
    }
}

impl ChainEntry for TestEntry {
    fn chain(&self) -> ChainId {
        self.chain
    }
    fn seq(&self) -> ChainSeq {
        self.seq
    }
    fn prev(&self) -> Hash32 {
        self.prev
    }
    fn hash(&self) -> Hash32 {
        self.hash
    }
    fn at(&self) -> Stamp {
        self.at
    }
    fn epoch(&self) -> RestoreEpoch {
        self.epoch
    }
    fn is_intact(&self) -> bool {
        self.hash
            == TestEntry::hash_of(
                &self.chain,
                self.seq,
                &self.prev,
                self.at,
                self.epoch,
                &self.payload,
            )
    }
}

/// A valid chain of `len` entries (at least 1) with non-decreasing stamps (equal ones included) and
/// non-decreasing restore epochs (a restore now and then: one in eight entries starts a new epoch).
pub fn build_chain(chain: ChainId, len: usize, rng: &mut Rng) -> Vec<TestEntry> {
    let mut out: Vec<TestEntry> = Vec::with_capacity(len);
    let mut now = 1_000_000 + rng.below(1000).unwrap_or(0);
    let mut restores = 0u64;
    for _ in 0..len {
        now += rng.below(50).unwrap_or(0);
        if rng.chance(1, 8) {
            restores += 1;
        }
        let size = rng.below(12).unwrap_or(0) as usize + 1;
        let payload = rng.bytes(size);
        let entry = match out.last() {
            None => TestEntry::new_in(
                chain,
                seq(0),
                Hash32::ZERO,
                at(now),
                RestoreEpoch::new(restores),
                &payload,
            ),
            Some(previous) => previous.after_in(at(now), RestoreEpoch::new(restores), &payload),
        };
        out.push(entry);
    }
    out
}

/// A valid chain of `len` entries with the stamps 100, 110, 120, ..., payload `e<i>` and the restore
/// epoch 0.
pub fn plain_chain(chain: ChainId, len: usize) -> Vec<TestEntry> {
    let mut out: Vec<TestEntry> = Vec::with_capacity(len);
    for i in 0..len {
        let payload = format!("e{i}");
        let stamp = at(100 + 10 * i as u64);
        let entry = match out.last() {
            None => TestEntry::genesis(chain, stamp, payload.as_bytes()),
            Some(previous) => previous.after(stamp, payload.as_bytes()),
        };
        out.push(entry);
    }
    out
}

// ---------------------------------------------------------------------------------------------
// Heads
// ---------------------------------------------------------------------------------------------

/// The bytes a test head signature covers: kind, id, position, hash, restore epoch, signing stamp.
pub fn head_bytes(head: &Head, epoch: RestoreEpoch, signed_at: Stamp) -> Vec<u8> {
    let mut out = vec![head.chain.kind as u8];
    out.extend_from_slice(&head.chain.id);
    out.extend_from_slice(&head.seq.get().to_be_bytes());
    out.extend_from_slice(head.hash.as_bytes());
    out.extend_from_slice(&epoch.get().to_be_bytes());
    out.extend_from_slice(&signed_at.as_secs().to_be_bytes());
    out
}

pub fn head_preimage(head: &Head, epoch: RestoreEpoch, signed_at: Stamp) -> Vec<u8> {
    signing_input(&HEAD_DOMAIN, &head_bytes(head, epoch, signed_at))
}

/// A head of the restore epoch 0 signed by the fake key `signer`.
pub fn sign_head(head: Head, signed_at: Stamp, signer: KeyId) -> SignedHead {
    sign_head_in(head, RestoreEpoch::ZERO, signed_at, signer)
}

/// A head of the given restore epoch signed by the fake key `signer`.
pub fn sign_head_in(
    head: Head,
    epoch: RestoreEpoch,
    signed_at: Stamp,
    signer: KeyId,
) -> SignedHead {
    let sig = FakeScheme::sign(&signer, &head_preimage(&head, epoch, signed_at));
    SignedHead {
        head,
        epoch,
        signed_at,
        signer,
        sig,
    }
}

/// A head of `chain` at position `n` whose hash is derived from `(n, variant)`: two variants of
/// one position are two different entries there (a fork).
pub fn head_at(chain: ChainId, n: u64, variant: u8) -> Head {
    Head {
        chain,
        seq: seq(n),
        hash: hash32(
            "azlin-chain-test-head-hash",
            &[&n.to_be_bytes(), &[variant], &chain.id],
        ),
    }
}

/// The group key of the tests.
pub fn group_key() -> KeyId {
    key(0x61)
}

/// The map key of the tests.
pub fn map_key() -> KeyId {
    key(0x4D)
}

/// A head of `chain` at `n`, restore epoch 0, signed by the group key at `n * 10` seconds.
pub fn group_head(chain: ChainId, n: u64, variant: u8) -> SignedHead {
    sign_head(
        head_at(chain, n, variant),
        at(n.saturating_mul(10)),
        group_key(),
    )
}

/// A head of `chain` at `n` signed by the group key in the restore epoch `in_epoch` at `signed_at`
/// seconds.
pub fn group_head_in(
    chain: ChainId,
    n: u64,
    variant: u8,
    in_epoch: u64,
    signed_at: u64,
) -> SignedHead {
    sign_head_in(
        head_at(chain, n, variant),
        RestoreEpoch::new(in_epoch),
        at(signed_at),
        group_key(),
    )
}

/// A head verifier over `FakeScheme` that trusts the given keys.
pub struct FakeHeadVerifier {
    trusted: Vec<KeyId>,
}

impl FakeHeadVerifier {
    pub fn trusting(keys: &[KeyId]) -> FakeHeadVerifier {
        FakeHeadVerifier {
            trusted: keys.to_vec(),
        }
    }

    /// The verifier of the tests: it trusts the group key.
    pub fn group() -> FakeHeadVerifier {
        FakeHeadVerifier::trusting(&[group_key()])
    }
}

impl HeadVerifier for FakeHeadVerifier {
    fn verify_head(&self, h: &SignedHead) -> Result<(), VerifyError> {
        if !self.trusted.contains(&h.signer) {
            return Err(VerifyError::BadKey);
        }
        FakeScheme.verify(
            SigAlg::Ed25519,
            &FakeScheme::public_key(&h.signer),
            &head_preimage(&h.head, h.epoch, h.signed_at),
            &h.sig,
        )
    }
}

/// A head verifier whose algorithm is unsupported: it never passes anything.
pub struct UnsupportedHeadVerifier;

impl HeadVerifier for UnsupportedHeadVerifier {
    fn verify_head(&self, _h: &SignedHead) -> Result<(), VerifyError> {
        Err(VerifyError::Unsupported(SigAlg::MlDsa65))
    }
}

// ---------------------------------------------------------------------------------------------
// Restore records
// ---------------------------------------------------------------------------------------------

/// The bytes a test restore signature covers: epoch, stamp, count and the positions (the
/// signatures themselves are not covered).
pub fn restore_bytes(rec: &RestoreRecord) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&rec.epoch.get().to_be_bytes());
    out.extend_from_slice(&rec.at.as_secs().to_be_bytes());
    out.extend_from_slice(&(rec.positions.len() as u32).to_be_bytes());
    for head in &rec.positions {
        out.extend_from_slice(&head_bytes(head, RestoreEpoch::ZERO, at(0)));
    }
    out
}

pub fn restore_preimage(rec: &RestoreRecord) -> Vec<u8> {
    signing_input(&RESTORE_DOMAIN, &restore_bytes(rec))
}

/// A signature of `rec` as it is now, by the fake key `key`, claimed for the role `by`.
pub fn restore_sig(rec: &RestoreRecord, by: RestoreSigner, key: KeyId) -> RestoreSig {
    RestoreSig {
        by,
        key,
        alg: SigAlg::Ed25519,
        sig: FakeScheme::sign(&key, &restore_preimage(rec)),
    }
}

/// A restore record of `positions` at the restore epoch `number`, stamped `5_000 + number`, with no
/// signature yet.
pub fn restore_record(number: u64, positions: Vec<Head>) -> RestoreRecord {
    restore_record_at(number, 5_000 + number, positions)
}

/// A restore record of `positions` at the restore epoch `number`, stamped `at_secs`, with no
/// signature yet.
pub fn restore_record_at(number: u64, at_secs: u64, positions: Vec<Head>) -> RestoreRecord {
    RestoreRecord {
        epoch: RestoreEpoch::new(number),
        at: at(at_secs),
        positions,
        signatures: Vec::new(),
    }
}

/// A record signed by the map key (and optionally the group).
pub fn map_signed(number: u64, positions: Vec<Head>, also_group: bool) -> RestoreRecord {
    sign_record(restore_record(number, positions), also_group)
}

/// A record stamped `at_secs`, signed by the map key.
pub fn map_signed_at(number: u64, at_secs: u64, positions: Vec<Head>) -> RestoreRecord {
    sign_record(restore_record_at(number, at_secs, positions), false)
}

fn sign_record(mut rec: RestoreRecord, also_group: bool) -> RestoreRecord {
    let mut signatures = vec![restore_sig(&rec, RestoreSigner::MapKey, map_key())];
    if also_group {
        signatures.push(restore_sig(&rec, RestoreSigner::Group, group_key()));
    }
    rec.signatures = signatures;
    rec
}

/// A record signed by the group alone.
pub fn group_signed(number: u64, positions: Vec<Head>) -> RestoreRecord {
    let mut rec = restore_record(number, positions);
    rec.signatures = vec![restore_sig(&rec, RestoreSigner::Group, group_key())];
    rec
}

/// The signature verifier of restore records over `FakeScheme`: the group key signs as `Group`, the
/// map key as `MapKey`, and a key claimed for the wrong role is a bad key.
pub struct FakeRestoreVerifier;

impl RestoreSigVerifier for FakeRestoreVerifier {
    fn verify_signature(&self, rec: &RestoreRecord, sig: &RestoreSig) -> Result<(), VerifyError> {
        let expected = match sig.by {
            RestoreSigner::Group => group_key(),
            RestoreSigner::MapKey => map_key(),
        };
        if sig.key != expected {
            return Err(VerifyError::BadKey);
        }
        FakeScheme.verify(
            sig.alg,
            &FakeScheme::public_key(&sig.key),
            &restore_preimage(rec),
            &sig.sig,
        )
    }
}

// ---------------------------------------------------------------------------------------------
// Compile-time witnesses of what a type is NOT
// ---------------------------------------------------------------------------------------------

/// `<T as AmbiguousIfClone<_>>::check()` compiles only when `T` is not `Clone`.
pub trait AmbiguousIfClone<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfClone<()> for T {}
impl<T: Clone> AmbiguousIfClone<u8> for T {}

/// The same for `Default`.
pub trait AmbiguousIfDefault<A> {
    fn check() {}
}
impl<T: ?Sized> AmbiguousIfDefault<()> for T {}
impl<T: Default> AmbiguousIfDefault<u8> for T {}
