//! The keyring of a world: every key is derived from (seed, entity), independent of the steps, and
//! never leaves the keyring except through a signer or a verifier handle (planning/architecture.md
//! section 5.9 "every entry is signed by a device whose role allows it", section 2.4 "the group's
//! identity key certifies a credential key", section 12 E4 "a restore is signed by the map key").
//!
//! The harness signs with TEST domains `azlin-sim-*` over its own preimages (rule H5, drift row
//! X-DELTA-W1SIM-9: the real byte-exact preimages of a head and a restore record are S5's and
//! CHAIN-GLUE's): they are never in the domain list of any crate and never used by production
//! code. The signature scheme is `azlin_seams::FakeScheme` (a keyed hash) or real Ed25519.

use azlin_chain::{
    Head, HeadVerifier, RestoreRecord, RestoreSig, RestoreSigVerifier, RestoreSigner, SignedHead,
};
use azlin_keys::sign::sign_preimage;
use azlin_keys::{Domain, Secret, SignKeypair};
use azlin_seams::{
    hash32, signing_input, Ed25519Verifier, FakeScheme, KeyId, RestoreEpoch, SigAlg,
    SignatureVerifier, Stamp, VerifyError,
};

use crate::config::Scheme;

/// The domain of a head's signature (a test domain).
///
/// Implements planning/architecture.md section 5.9 (a signed head); drift row X-DELTA-W1SIM-9.
///
/// Caller: `Keyring::sign_head`, `SimHeadVerifier`.
pub const HEAD_DOMAIN: Domain = Domain::new("azlin-sim-head-v1");

/// The domain of a restore record's signature (a test domain).
///
/// Implements planning/architecture.md section 2.17 and section 12 E4; drift row X-DELTA-W1SIM-9.
///
/// Caller: `Keyring::restore_sig`, `SimRestoreVerifier`.
pub const RESTORE_DOMAIN: Domain = Domain::new("azlin-sim-restore-v1");

/// The domain of a device's signature over a proposal (a test domain).
///
/// Implements planning/architecture.md section 5.9 (every entry is signed by a device); drift row
/// X-DELTA-W1SIM-9.
///
/// Caller: `machine::OpaqueMachine`.
pub const ENTRY_DOMAIN: Domain = Domain::new("azlin-sim-entry-v1");

/// The domain of the harness's key derivation.
const KEY_DOMAIN: &str = "azlin-sim-key-v1";

/// The domains the harness signs under (rule H5: all of them are `azlin-sim-*`).
///
/// Implements planning/architecture.md section 2.5 (a domain string per signed structure).
///
/// Caller: tests/hygiene.rs.
pub const HARNESS_DOMAINS: [Domain; 3] = [HEAD_DOMAIN, RESTORE_DOMAIN, ENTRY_DOMAIN];

/// The bytes a head's signature covers: kind, id, position, hash, restore epoch, signing stamp.
///
/// Implements planning/architecture.md section 6.10 V3 (a restore epoch the metadata group signs).
///
/// Caller: `Keyring::sign_head`, `SimHeadVerifier`, `adversary`.
pub fn head_bytes(head: &Head, epoch: RestoreEpoch, signed_at: Stamp) -> Vec<u8> {
    let mut out = vec![head.chain.kind as u8];
    out.extend_from_slice(&head.chain.id);
    out.extend_from_slice(&head.seq.get().to_be_bytes());
    out.extend_from_slice(head.hash.as_bytes());
    out.extend_from_slice(&epoch.get().to_be_bytes());
    out.extend_from_slice(&signed_at.as_secs().to_be_bytes());
    out
}

/// The bytes a restore signature covers: epoch, stamp, count and the positions (the signatures
/// themselves are not covered).
///
/// Implements planning/architecture.md section 2.17 (the restore record lists the positions that
/// survived).
///
/// Caller: `Keyring::restore_sig`, `SimRestoreVerifier`, `leader`.
pub fn restore_bytes(rec: &RestoreRecord) -> Vec<u8> {
    let mut out = Vec::new();
    out.extend_from_slice(&rec.epoch.get().to_be_bytes());
    out.extend_from_slice(&rec.at.as_secs().to_be_bytes());
    out.extend_from_slice(&(rec.positions.len() as u64).to_be_bytes());
    for head in &rec.positions {
        out.push(head.chain.kind as u8);
        out.extend_from_slice(&head.chain.id);
        out.extend_from_slice(&head.seq.get().to_be_bytes());
        out.extend_from_slice(head.hash.as_bytes());
    }
    out
}

/// A restore record's identity: the hash of its signed bytes (two records with the same bytes are
/// one record, whoever signed them).
///
/// Implements planning/architecture.md section 2.17 (a restore record).
///
/// Caller: `invariants` (the set of records the operator signed), `leader`.
pub fn record_id(rec: &RestoreRecord) -> azlin_seams::Hash32 {
    hash32("azlin-sim-record-id-v1", &[&restore_bytes(rec)])
}

/// One signing key: the keypair of a real scheme, or the key id of the fake one.
enum Signer {
    Fake(KeyId),
    Real(SignKeypair, KeyId),
}

impl Signer {
    fn key_id(&self) -> KeyId {
        match self {
            Signer::Fake(id) | Signer::Real(_, id) => *id,
        }
    }

    fn sign(&self, preimage: &[u8]) -> Vec<u8> {
        match self {
            Signer::Fake(id) => FakeScheme::sign(id, preimage),
            Signer::Real(pair, _) => sign_preimage(preimage, pair).as_bytes().to_vec(),
        }
    }
}

/// The keys of a world: one for each device, the group key (the metadata group's credential key)
/// and the map key (the operator's). Derived from the seed; `Debug` prints nothing secret.
///
/// Implements planning/architecture.md section 5.9, section 2.4 and section 12 E4; rule H9 of the
/// harness.
///
/// Caller: `world::World`, `machine::OpaqueMachine`, tests/planted_*.
pub struct Keyring {
    scheme: Scheme,
    devices: Vec<Signer>,
    group: Signer,
    map: Signer,
}

impl core::fmt::Debug for Keyring {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "Keyring({} devices, keys ***)", self.devices.len())
    }
}

impl Keyring {
    /// The keys of `devices` devices for this seed, under this scheme.
    pub fn new(seed: u64, scheme: Scheme, devices: usize) -> Keyring {
        let derive = |kind: u8, index: u64| -> Signer {
            let digest = hash32(
                KEY_DOMAIN,
                &[&seed.to_le_bytes(), &[kind], &index.to_le_bytes()],
            );
            match scheme {
                Scheme::Fake => Signer::Fake(KeyId::from_bytes(*digest.as_bytes())),
                Scheme::Ed25519 => {
                    let pair = SignKeypair::from_seed(&Secret::from_bytes(*digest.as_bytes()));
                    let id = KeyId::from_bytes(*pair.verify_key().as_bytes());
                    Signer::Real(pair, id)
                }
            }
        };
        Keyring {
            scheme,
            devices: (0..devices as u64).map(|i| derive(1, i)).collect(),
            group: derive(2, 0),
            map: derive(3, 0),
        }
    }

    /// The scheme.
    pub fn scheme(&self) -> Scheme {
        self.scheme
    }

    /// How many device keys there are.
    pub fn device_count(&self) -> usize {
        self.devices.len()
    }

    /// The public key id of device `index`.
    pub fn device_key(&self, index: usize) -> Option<KeyId> {
        self.devices.get(index).map(Signer::key_id)
    }

    /// The public key id of the group (the metadata group's credential key).
    pub fn group_key(&self) -> KeyId {
        self.group.key_id()
    }

    /// The public key id of the map key (the operator's).
    pub fn map_key(&self) -> KeyId {
        self.map.key_id()
    }

    /// Device `index` signs `bytes` under `domain`.
    pub fn sign_device(&self, index: usize, domain: &Domain, bytes: &[u8]) -> Option<Vec<u8>> {
        let signer = self.devices.get(index)?;
        Some(signer.sign(&signing_input(domain, bytes)))
    }

    /// The group signs a head: the head, the restore epoch it was written in and the stamp.
    pub fn sign_head(&self, head: Head, epoch: RestoreEpoch, signed_at: Stamp) -> SignedHead {
        let sig = self.group.sign(&signing_input(
            &HEAD_DOMAIN,
            &head_bytes(&head, epoch, signed_at),
        ));
        SignedHead {
            head,
            epoch,
            signed_at,
            signer: self.group.key_id(),
            sig,
        }
    }

    /// A signature of `rec` as it is now, by the group key or the map key.
    pub fn restore_sig(&self, rec: &RestoreRecord, by: RestoreSigner) -> RestoreSig {
        let signer = match by {
            RestoreSigner::Group => &self.group,
            RestoreSigner::MapKey => &self.map,
        };
        RestoreSig {
            by,
            key: signer.key_id(),
            alg: SigAlg::Ed25519,
            sig: signer.sign(&signing_input(&RESTORE_DOMAIN, &restore_bytes(rec))),
        }
    }

    /// Whether `sig` is a signature of the framed `preimage` under the public key `key`, by the
    /// scheme of this keyring.
    pub fn verify(&self, key: &KeyId, preimage: &[u8], sig: &[u8]) -> Result<(), VerifyError> {
        match self.scheme {
            Scheme::Fake => FakeScheme.verify(SigAlg::Ed25519, key.as_bytes(), preimage, sig),
            Scheme::Ed25519 => {
                Ed25519Verifier.verify(SigAlg::Ed25519, key.as_bytes(), preimage, sig)
            }
        }
    }

    /// Whether device `key` signed `bytes` under `domain`.
    pub fn verify_device(
        &self,
        key: &KeyId,
        domain: &Domain,
        bytes: &[u8],
        sig: &[u8],
    ) -> Result<(), VerifyError> {
        self.verify(key, &signing_input(domain, bytes), sig)
    }
}

/// The verifier of signed heads: the head must be signed by the group key. With `accept_all` it
/// accepts every head (the faulty double of the planted tests).
///
/// Implements planning/architecture.md section 5.9 (a head the metadata group signed).
///
/// Caller: `device::Device`, `adversary`, `invariants`, tests/planted_*.
pub struct SimHeadVerifier<'a> {
    keys: &'a Keyring,
    accept_all: bool,
}

impl<'a> SimHeadVerifier<'a> {
    /// The verifier of the keyring.
    pub fn new(keys: &'a Keyring) -> SimHeadVerifier<'a> {
        SimHeadVerifier {
            keys,
            accept_all: false,
        }
    }

    /// A verifier that accepts every head (a faulty double).
    pub fn accepting_everything(keys: &'a Keyring) -> SimHeadVerifier<'a> {
        SimHeadVerifier {
            keys,
            accept_all: true,
        }
    }
}

impl HeadVerifier for SimHeadVerifier<'_> {
    fn verify_head(&self, h: &SignedHead) -> Result<(), VerifyError> {
        if self.accept_all {
            return Ok(());
        }
        if h.signer != self.keys.group_key() {
            return Err(VerifyError::BadKey);
        }
        self.keys.verify(
            &h.signer,
            &signing_input(&HEAD_DOMAIN, &head_bytes(&h.head, h.epoch, h.signed_at)),
            &h.sig,
        )
    }
}

/// The verifier of the signatures of a restore record: the group key signs as `Group`, the map key
/// as `MapKey`, and a key claimed for the wrong role is a bad key.
///
/// Implements planning/architecture.md section 12 E4 and section 2.17.
///
/// Caller: `device::Device`, `leader`, `adversary`, `invariants`.
pub struct SimRestoreVerifier<'a> {
    keys: &'a Keyring,
}

impl<'a> SimRestoreVerifier<'a> {
    /// The verifier of the keyring.
    pub fn new(keys: &'a Keyring) -> SimRestoreVerifier<'a> {
        SimRestoreVerifier { keys }
    }
}

impl RestoreSigVerifier for SimRestoreVerifier<'_> {
    fn verify_signature(&self, rec: &RestoreRecord, sig: &RestoreSig) -> Result<(), VerifyError> {
        let expected = match sig.by {
            RestoreSigner::Group => self.keys.group_key(),
            RestoreSigner::MapKey => self.keys.map_key(),
        };
        if sig.key != expected {
            return Err(VerifyError::BadKey);
        }
        if sig.alg != SigAlg::Ed25519 {
            return Err(VerifyError::Unsupported(sig.alg));
        }
        self.keys.verify(
            &sig.key,
            &signing_input(&RESTORE_DOMAIN, &restore_bytes(rec)),
            &sig.sig,
        )
    }
}
