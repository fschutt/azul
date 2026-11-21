//! Helpers the test files share: typed ids and stamps, a table-driven registry (`SpaceAccess`), signed
//! log records over `FakeScheme`, restore requests and verified restore records (the witnesses), and
//! the compile-time proofs of what a type is NOT.
//!
//! The signed bytes of a restore request and of a restore record are the TEST's own (a fixed layout
//! under a test domain): the real ones are S43's and the glue's, and this crate never builds them.
#![allow(dead_code)]

use std::collections::{BTreeMap, BTreeSet};

use azlin_chain::{
    verify_record, ChainId, Head, RestoreRecord, RestoreSig, RestoreSigVerifier, RestoreSigner,
    StrictTrust, VerifiedRestore,
};
use azlin_seams::{
    signing_input, AccountId, ChainSeq, DeviceId, Domain, FakeScheme, Hash32, KeyId, RestoreEpoch,
    SigAlg, SignatureVerifier, SpaceId, Stamp, UnixTime, VerifyError, VerifyKey,
};
use azlin_sync::{
    record_preimage, HeadRegister, HeadValue, LockTerm, LogId, Record, Requester, RestoreAuth,
    RestoreRequest, RestoreRequestVerifier, Seq, SpaceAccess, Version,
};

// ---------------------------------------------------------------------------------------------
// Typed values
// ---------------------------------------------------------------------------------------------

pub fn at(secs: u64) -> Stamp {
    UnixTime::from_secs(secs)
}

pub fn space(n: u8) -> SpaceId {
    SpaceId::from_bytes([n; 16])
}

pub fn device(n: u8) -> DeviceId {
    DeviceId::from_bytes([n; 16])
}

pub fn account(n: u8) -> AccountId {
    AccountId::from_bytes([n; 16])
}

pub fn version(n: u64) -> Version {
    Version::new(n).expect("a test version is not zero")
}

pub fn seq(n: u64) -> Seq {
    Seq::new(n).expect("a test sequence number is not zero")
}

pub fn epoch(n: u64) -> RestoreEpoch {
    RestoreEpoch::new(n)
}

pub fn value(text: &str) -> HeadValue {
    HeadValue::new(text.as_bytes().to_vec())
}

pub fn log_id(space_n: u8, name_n: u8) -> LogId {
    LogId {
        space: space(space_n),
        name: [name_n; 16],
    }
}

pub fn hash_of_fill(fill: u8) -> Hash32 {
    Hash32::from_bytes([fill; 32])
}

/// The fake key of a device: its signing key in the tests.
pub fn device_key(n: u8) -> KeyId {
    KeyId::from_bytes([n ^ 0x5A; 32])
}

// ---------------------------------------------------------------------------------------------
// A register helper
// ---------------------------------------------------------------------------------------------

/// Writes `texts` to the register, one after another, each expecting the head, at the stamps `10, 20,
/// ...` by the device 1: the first CAS creates the register.
pub fn fill(register: &mut HeadRegister, texts: &[&str]) {
    for (i, text) in texts.iter().enumerate() {
        let expected = register.get().map(|record| record.version);
        let stamp = at(10 * (i as u64 + 1));
        let outcome = register.cas(expected, value(text), device(1), stamp, LockTerm::Unlocked);
        assert!(
            matches!(outcome, azlin_sync::CasOutcome::Ok { .. }),
            "fill: {outcome:?}"
        );
    }
}

// ---------------------------------------------------------------------------------------------
// The registry's view
// ---------------------------------------------------------------------------------------------

/// A table-driven `SpaceAccess`.
#[derive(Default)]
pub struct FakeAccess {
    pub readers: BTreeSet<(DeviceId, SpaceId)>,
    pub writers: BTreeSet<(DeviceId, SpaceId)>,
    pub keys: BTreeMap<DeviceId, KeyId>,
}

impl FakeAccess {
    /// Devices `devices` may read and append to `space_n`, each with its fake key.
    pub fn open(space_n: u8, devices: &[u8]) -> FakeAccess {
        let mut access = FakeAccess::default();
        for n in devices {
            access.readers.insert((device(*n), space(space_n)));
            access.writers.insert((device(*n), space(space_n)));
            access.keys.insert(device(*n), device_key(*n));
        }
        access
    }
}

impl SpaceAccess for FakeAccess {
    fn may_head_get(&self, device: DeviceId, space: SpaceId) -> bool {
        self.readers.contains(&(device, space))
    }

    fn may_append(&self, device: DeviceId, space: SpaceId) -> bool {
        self.writers.contains(&(device, space))
    }

    fn author_key(&self, device: DeviceId) -> Option<VerifyKey> {
        self.keys
            .get(&device)
            .map(|key| VerifyKey::from_bytes(*key.as_bytes()))
    }
}

/// A record of the log `log` by the device `device_n`, signed with its fake key.
pub fn signed(log: &LogId, device_n: u8, body: &[u8]) -> Record {
    let author = device(device_n);
    let preimage = record_preimage(log, author, body);
    Record {
        author,
        body: body.to_vec(),
        sig: FakeScheme::sign(&device_key(device_n), &preimage),
    }
}

// ---------------------------------------------------------------------------------------------
// Restore requests (the witness of restore_as_of)
// ---------------------------------------------------------------------------------------------

pub const REQUEST_DOMAIN: Domain = Domain::new("azlin-sync-test-restore-request-v1");

pub fn operator_key() -> KeyId {
    KeyId::from_bytes([0x0A; 32])
}

pub fn request_preimage(space: SpaceId, as_of: Stamp) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(space.as_bytes());
    bytes.extend_from_slice(&as_of.as_secs().to_be_bytes());
    signing_input(&REQUEST_DOMAIN, &bytes)
}

/// A request for `space` as of `as_of`, signed by the fake key `by`.
pub fn signed_request(space: SpaceId, as_of: Stamp, by: KeyId) -> RestoreRequest {
    RestoreRequest {
        space,
        as_of,
        sig: FakeScheme::sign(&by, &request_preimage(space, as_of)),
    }
}

/// The verifier of the tests: the operator's key makes an `Operator`, an owner device's key an
/// `Owner`; anything else is refused.
pub struct FakeRequestVerifier {
    pub owners: Vec<(DeviceId, KeyId)>,
}

impl RestoreRequestVerifier for FakeRequestVerifier {
    fn verify_request(&self, req: &RestoreRequest) -> Result<Requester, VerifyError> {
        let preimage = request_preimage(req.space, req.as_of);
        if FakeScheme
            .verify(
                SigAlg::Ed25519,
                &FakeScheme::public_key(&operator_key()),
                &preimage,
                &req.sig,
            )
            .is_ok()
        {
            return Ok(Requester::Operator);
        }
        for (device, key) in &self.owners {
            if FakeScheme
                .verify(
                    SigAlg::Ed25519,
                    &FakeScheme::public_key(key),
                    &preimage,
                    &req.sig,
                )
                .is_ok()
            {
                return Ok(Requester::Owner(*device));
            }
        }
        Err(VerifyError::BadSignature)
    }
}

/// The witness of an operator's restore of `space` as of `as_of` that carries the epoch `epoch_n`.
pub fn operator_auth(space: SpaceId, as_of: Stamp, epoch_n: u64) -> RestoreAuth {
    let request = signed_request(space, as_of, operator_key());
    RestoreAuth::verify(
        &request,
        epoch(epoch_n),
        &FakeRequestVerifier { owners: Vec::new() },
    )
    .expect("the operator's request verifies")
}

/// The witness of an owner's restore (through the device `owner_n`).
pub fn owner_auth(space: SpaceId, as_of: Stamp, epoch_n: u64, owner_n: u8) -> RestoreAuth {
    let request = signed_request(space, as_of, device_key(owner_n));
    RestoreAuth::verify(
        &request,
        epoch(epoch_n),
        &FakeRequestVerifier {
            owners: vec![(device(owner_n), device_key(owner_n))],
        },
    )
    .expect("the owner's request verifies")
}

// ---------------------------------------------------------------------------------------------
// Restore records (the witness of the device guard)
// ---------------------------------------------------------------------------------------------

pub const RESTORE_DOMAIN: Domain = Domain::new("azlin-sync-test-restore-v1");

pub fn map_key() -> KeyId {
    KeyId::from_bytes([0x4D; 32])
}

pub fn group_key() -> KeyId {
    KeyId::from_bytes([0x61; 32])
}

pub fn restore_bytes(rec: &RestoreRecord) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&rec.epoch.get().to_be_bytes());
    out.extend_from_slice(&rec.at.as_secs().to_be_bytes());
    out.extend_from_slice(&(rec.positions.len() as u32).to_be_bytes());
    for head in &rec.positions {
        out.push(head.chain.kind as u8);
        out.extend_from_slice(&head.chain.id);
        out.extend_from_slice(&head.seq.get().to_be_bytes());
        out.extend_from_slice(head.hash.as_bytes());
    }
    out
}

pub fn restore_preimage(rec: &RestoreRecord) -> Vec<u8> {
    signing_input(&RESTORE_DOMAIN, &restore_bytes(rec))
}

pub fn restore_sig(rec: &RestoreRecord, by: RestoreSigner, key: KeyId) -> RestoreSig {
    RestoreSig {
        by,
        key,
        alg: SigAlg::Ed25519,
        sig: FakeScheme::sign(&key, &restore_preimage(rec)),
    }
}

/// A restore record of the restore epoch `epoch_n` at `at_secs` that lists the spaces `spaces` (a
/// position each), with no signature yet.
pub fn restore_record(epoch_n: u64, at_secs: u64, spaces: &[u8]) -> RestoreRecord {
    RestoreRecord {
        epoch: epoch(epoch_n),
        at: at(at_secs),
        positions: spaces
            .iter()
            .map(|n| Head {
                chain: ChainId::space(space(*n)),
                seq: ChainSeq::new(1),
                hash: hash_of_fill(*n),
            })
            .collect(),
        signatures: Vec::new(),
    }
}

/// The record signed by the map key.
pub fn map_signed(mut rec: RestoreRecord) -> RestoreRecord {
    rec.signatures = vec![restore_sig(&rec, RestoreSigner::MapKey, map_key())];
    rec
}

/// The record signed by the group alone (the attack of E4).
pub fn group_signed(mut rec: RestoreRecord) -> RestoreRecord {
    rec.signatures = vec![restore_sig(&rec, RestoreSigner::Group, group_key())];
    rec
}

/// The signature verifier of restore records over `FakeScheme`.
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

/// The witness of a restore of epoch `epoch_n` at `at_secs` that covers `spaces`, signed by the map
/// key and verified under the strict trust.
pub fn verified(epoch_n: u64, at_secs: u64, spaces: &[u8]) -> VerifiedRestore {
    let rec = map_signed(restore_record(epoch_n, at_secs, spaces));
    verify_record(&rec, &StrictTrust::new(&FakeRestoreVerifier))
        .expect("a map-signed record verifies under the strict trust")
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
