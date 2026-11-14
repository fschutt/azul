//! The wire format of envelope v2: strict deterministic CBOR (`azlin-cbor`), `Envelope`,
//! `Params`, the associated data and the signing input (planning/architecture.md section 5.5,
//! "Envelope v2 (every wrap)"; section 2.5; build unit S15, lane W1-S15b).
//!
//! An envelope is a CBOR map with unsigned-integer keys (key 0 is `crit`, as in every struct of
//! section 5):
//!
//! | key | field | type |
//! |---|---|---|
//! | 1 | `v` | 2 |
//! | 2 | `alg` | 1 to 7 |
//! | 3 | `rcpt` | 32 bytes: BLAKE3 of the recipient's public keys (`KeyId::of_bundle`) |
//! | 4 | `epk` | 32 bytes: the ephemeral public key (algorithms 1, 2, 7; 5 and 6 optionally) |
//! | 5 | `kem_ct` | 1,088 bytes: the ML-KEM-768 ciphertext (algorithm 2; 5 optionally) |
//! | 6 | `nonce` | 24 bytes |
//! | 7 | `ct` | the XChaCha20-Poly1305 ciphertext and its 16-byte tag |
//! | 8 | `ctx` | text: what is wrapped |
//! | 9 | `signer` | 16 bytes: the wrapping device |
//! | 10 | `sig` | 64 bytes: Ed25519 over `azlin-envelope-v2 0x00` and the canonical bytes without `sig` |
//! | 16 | `params` | a closed map of the algorithms 3 to 7 |
//!
//! The numbers of the keys 1 to 10 follow the order of the plan's table; key 16 is the brief's
//! (DRIFT X-DELTA-W1S15B-1, -7). Every other key is unknown: ignored unless `crit` lists it, and
//! KEPT in [`Envelope::unknown`] so that a copy writes the same bytes. A copy keeps `crit` too
//! (rule R18: [`Envelope::crit`]).
//!
//! THE ASSOCIATED DATA of the AEAD is the canonical CBOR of the same map reduced to the header
//! fields `v, alg, rcpt, epk, kem_ct, ctx, signer` (and `params` when the algorithm has them):
//! every non-secret field the ciphertext depends on (DRIFT X-DELTA-W1S15B-3). THE SIGNATURE
//! covers the whole map without `sig`, so the signer vouches for the ciphertext, the nonce and
//! the unknown fields too.
//!
//! Every decode goes through `azlin_cbor::decode_secret` (rule R17): the fields of an envelope
//! are public, but the `params` of algorithms 3 to 7 belong to lanes whose other formats carry
//! key material, and wiping the decoded buffers costs a few microseconds (DRIFT
//! X-DELTA-W1S15B-15).

use std::collections::BTreeSet;

use azlin_cbor::{
    decode_secret, encode, IntMap, IntMapBuilder, Limits, MapError, ShapeError, Value,
};

use super::aead::TAG_LEN;
use super::alg_table::{rules, Alg, ParamsKind, Presence};
use super::ctx::Ctx;
use super::error::{EnvelopeError, ParamsError};
use crate::ids::{DeviceId, KeyId};
use crate::sign::{self, SignKeypair, Signature, ENVELOPE_V2};

/// The integer keys of the envelope map.
pub mod field {
    /// `v`: the version, 2.
    pub const V: u64 = 1;
    /// `alg`: the algorithm, 1 to 7.
    pub const ALG: u64 = 2;
    /// `rcpt`: the key id of the recipient.
    pub const RCPT: u64 = 3;
    /// `epk`: the ephemeral public key.
    pub const EPK: u64 = 4;
    /// `kem_ct`: the ML-KEM-768 ciphertext.
    pub const KEM_CT: u64 = 5;
    /// `nonce`: the AEAD nonce.
    pub const NONCE: u64 = 6;
    /// `ct`: the ciphertext and its tag.
    pub const CT: u64 = 7;
    /// `ctx`: what is wrapped.
    pub const CTX: u64 = 8;
    /// `signer`: the wrapping device.
    pub const SIGNER: u64 = 9;
    /// `sig`: the signature of the wrapping device.
    pub const SIG: u64 = 10;
    /// `params`: the parameters of the algorithms 3 to 7.
    pub const PARAMS: u64 = 16;
    /// Every key this version understands (key 0, `crit`, is understood by the codec).
    pub const KNOWN: [u64; 11] = [
        V, ALG, RCPT, EPK, KEM_CT, NONCE, CT, CTX, SIGNER, SIG, PARAMS,
    ];
}

/// The longest plaintext a wrap carries: 1 MiB. Everything has a limit, stated up front
/// (planning/architecture.md section 1.4 rule 4). A wrap kept in the registry is far smaller
/// ([`MAX_ENVELOPE`](super::verify::MAX_ENVELOPE)); this limit only bounds memory.
///
/// Caller: `wrap_x25519`, `wrap_hybrid`, `seal_under`, the wire decoder.
pub const MAX_PLAINTEXT: usize = 1_048_576;

/// The length of the ML-KEM-768 ciphertext of algorithm 2, in bytes.
///
/// Caller: the wire decoder, `wrap_hybrid`.
pub const KEM_CT_LEN: usize = 1_088;

/// The limits of reading and writing an envelope: the plaintext limit plus 8 KiB for the header
/// and the parameters, depth 4, 64 items per container and 512 in all. DRIFT X-DELTA-W1S15B-9.
///
/// Caller: `Envelope::to_cbor`, `Envelope::from_cbor`, the associated data.
pub const WIRE_LIMITS: Limits = Limits {
    max_input: 1_048_576 + 8_192,
    max_depth: 4,
    max_items: 64,
    max_total_items: 512,
    max_text: 1_048_576 + 8_192,
};

/// The most memory (in KiB) the Argon2id parameters of algorithm 3 may ask for: 4 GiB.
///
/// Caller: `Params::validate` (the shape check of the node); W1-S15d applies its own, tighter
/// ceiling before it hashes anything.
pub const MAX_ARGON2_M_KIB: u32 = 4_194_304;

/// The most passes the Argon2id parameters of algorithm 3 may ask for.
///
/// Caller: `Params::validate`.
pub const MAX_ARGON2_T: u32 = 1_024;

/// The most lanes the Argon2id parameters of algorithm 3 may ask for.
///
/// Caller: `Params::validate`.
pub const MAX_ARGON2_P: u32 = 255;

/// The longest `evidence` of algorithm 6, in bytes.
///
/// Caller: `Params::validate`; S17 (algorithm 6).
pub const MAX_EVIDENCE_LEN: usize = 2_048;

/// The longest `policy` of algorithm 6, in bytes.
///
/// Caller: `Params::validate`; S17 (algorithm 6).
pub const MAX_POLICY_LEN: usize = 256;

/// The longest `holder` of algorithm 7, in bytes.
///
/// Caller: `Params::validate`; S17 (algorithm 7).
pub const MAX_HOLDER_LEN: usize = 64;

/// The integer keys inside the `params` maps (each algorithm numbers its own, from 1).
mod pk {
    pub(super) const ARGON_SALT: u64 = 1;
    pub(super) const ARGON_M_KIB: u64 = 2;
    pub(super) const ARGON_T: u64 = 3;
    pub(super) const ARGON_P: u64 = 4;
    pub(super) const PRF_SALT: u64 = 1;
    pub(super) const PRF_CRED: u64 = 2;
    pub(super) const SHAMIR_X: u64 = 1;
    pub(super) const SHAMIR_K: u64 = 2;
    pub(super) const SHAMIR_N: u64 = 3;
    pub(super) const SHAMIR_SPLIT: u64 = 4;
    pub(super) const ATTESTED_EVIDENCE: u64 = 1;
    pub(super) const ATTESTED_POLICY: u64 = 2;
    pub(super) const THRESHOLD_K: u64 = 1;
    pub(super) const THRESHOLD_N: u64 = 2;
    pub(super) const THRESHOLD_HOLDER: u64 = 3;
}

/// The algorithm-specific parameters of an envelope (the `params` map under key 16, P-2 of the
/// brief, DRIFT X-DELTA-W1S15B-1). The maps are CLOSED: exactly the keys listed here, each of
/// its own type and range; an unknown key refuses the envelope (a new parameter needs a new
/// version, because a parameter an older reader skips could change what a key derivation
/// means).
///
/// Caller: `Envelope::params`, S15d (algorithms 3, 4 and 5), S17 (algorithms 6 and 7),
/// `check_shape` (INT-S15-2).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Params {
    /// Algorithms 1 and 2: no `params`.
    None,
    /// Algorithm 3: the Argon2id cost and salt of a recovery code
    /// (`{1: salt, 2: m_kib, 3: t, 4: p}`).
    Argon2id {
        /// The salt: 16 bytes.
        salt: [u8; 16],
        /// The memory cost in KiB (8 times `p` to [`MAX_ARGON2_M_KIB`]).
        m_kib: u32,
        /// The number of passes (1 to [`MAX_ARGON2_T`]).
        t: u32,
        /// The number of lanes (1 to [`MAX_ARGON2_P`]).
        p: u32,
    },
    /// Algorithm 4: the salt of the passkey PRF and the hash of the credential id
    /// (`{1: salt, 2: cred_id_hash}`).
    Prf {
        /// The PRF salt: 32 bytes.
        salt: [u8; 32],
        /// The hash of the credential id: 32 bytes.
        cred_id_hash: [u8; 32],
    },
    /// Algorithm 5: which share of which split this is (`{1: x, 2: k, 3: n, 4: split_id}`).
    Shamir {
        /// The index of the share (1 to 255; 0 is the secret).
        x: u8,
        /// The threshold: shares needed (1 to `n`).
        k: u8,
        /// The number of shares.
        n: u8,
        /// The id of the split: 4 bytes.
        split_id: [u8; 4],
    },
    /// Algorithm 6: the evidence about the attested key and the policy it was checked against
    /// (`{1: evidence, 2: policy}`). Shape only here: S17 (DRIFT X-DELTA-W1S15B-11).
    Attested {
        /// The evidence: 1 to [`MAX_EVIDENCE_LEN`] bytes.
        evidence: Vec<u8>,
        /// The policy: 1 to [`MAX_POLICY_LEN`] bytes.
        policy: Vec<u8>,
    },
    /// Algorithm 7: the threshold of the escrow and the holder (`{1: k, 2: n, 3: holder}`).
    /// Shape only here: S17 (DRIFT X-DELTA-W1S15B-11).
    Threshold {
        /// The threshold (1 to `n`).
        k: u8,
        /// The number of holders.
        n: u8,
        /// The holder: 1 to [`MAX_HOLDER_LEN`] bytes.
        holder: Vec<u8>,
    },
}

/// `n` as the smaller integer type of a parameter, or the parameter's key when it does not fit.
fn narrow<T: TryFrom<u64>>(key: u64, n: u64) -> Result<T, ParamsError> {
    T::try_from(n).map_err(|_| ParamsError::OutOfRange(key))
}

/// Whether `lo..=hi` holds `len`; the parameter's key otherwise.
fn length_in(key: u64, len: usize, lo: usize, hi: usize) -> Result<(), ParamsError> {
    if (lo..=hi).contains(&len) {
        Ok(())
    } else {
        Err(ParamsError::OutOfRange(key))
    }
}

impl Params {
    /// Which kind of `params` these are.
    pub fn kind(&self) -> ParamsKind {
        match self {
            Params::None => ParamsKind::None,
            Params::Argon2id { .. } => ParamsKind::Argon2id,
            Params::Prf { .. } => ParamsKind::Prf,
            Params::Shamir { .. } => ParamsKind::Shamir,
            Params::Attested { .. } => ParamsKind::Attested,
            Params::Threshold { .. } => ParamsKind::Threshold,
        }
    }

    /// The ranges of the values: what an honest writer never leaves and a hostile server could.
    pub(crate) fn validate(&self) -> Result<(), ParamsError> {
        match self {
            Params::None | Params::Prf { .. } => Ok(()),
            Params::Argon2id { m_kib, t, p, .. } => {
                if !(1..=MAX_ARGON2_P).contains(p) {
                    return Err(ParamsError::OutOfRange(pk::ARGON_P));
                }
                if !(1..=MAX_ARGON2_T).contains(t) {
                    return Err(ParamsError::OutOfRange(pk::ARGON_T));
                }
                // Argon2 needs at least 8 KiB for each lane.
                let floor = p.saturating_mul(8);
                if *m_kib < floor || *m_kib > MAX_ARGON2_M_KIB {
                    return Err(ParamsError::OutOfRange(pk::ARGON_M_KIB));
                }
                Ok(())
            }
            Params::Shamir { x, k, n, .. } => {
                if *x == 0 {
                    return Err(ParamsError::OutOfRange(pk::SHAMIR_X));
                }
                if *k == 0 || k > n {
                    return Err(ParamsError::OutOfRange(pk::SHAMIR_K));
                }
                Ok(())
            }
            Params::Attested { evidence, policy } => {
                length_in(pk::ATTESTED_EVIDENCE, evidence.len(), 1, MAX_EVIDENCE_LEN)?;
                length_in(pk::ATTESTED_POLICY, policy.len(), 1, MAX_POLICY_LEN)
            }
            Params::Threshold { k, n, holder } => {
                if *k == 0 || k > n {
                    return Err(ParamsError::OutOfRange(pk::THRESHOLD_K));
                }
                length_in(pk::THRESHOLD_HOLDER, holder.len(), 1, MAX_HOLDER_LEN)
            }
        }
    }

    /// The `params` map, or `None` when there are none.
    fn to_value(&self) -> Result<Option<Value>, EnvelopeError> {
        let builder = IntMapBuilder::new();
        let built = match self {
            Params::None => return Ok(None),
            Params::Argon2id { salt, m_kib, t, p } => builder
                .put(pk::ARGON_SALT, Value::Bytes(salt.to_vec()))
                .put(pk::ARGON_M_KIB, Value::Uint(u64::from(*m_kib)))
                .put(pk::ARGON_T, Value::Uint(u64::from(*t)))
                .put(pk::ARGON_P, Value::Uint(u64::from(*p))),
            Params::Prf { salt, cred_id_hash } => builder
                .put(pk::PRF_SALT, Value::Bytes(salt.to_vec()))
                .put(pk::PRF_CRED, Value::Bytes(cred_id_hash.to_vec())),
            Params::Shamir { x, k, n, split_id } => builder
                .put(pk::SHAMIR_X, Value::Uint(u64::from(*x)))
                .put(pk::SHAMIR_K, Value::Uint(u64::from(*k)))
                .put(pk::SHAMIR_N, Value::Uint(u64::from(*n)))
                .put(pk::SHAMIR_SPLIT, Value::Bytes(split_id.to_vec())),
            Params::Attested { evidence, policy } => builder
                .put(pk::ATTESTED_EVIDENCE, Value::Bytes(evidence.clone()))
                .put(pk::ATTESTED_POLICY, Value::Bytes(policy.clone())),
            Params::Threshold { k, n, holder } => builder
                .put(pk::THRESHOLD_K, Value::Uint(u64::from(*k)))
                .put(pk::THRESHOLD_N, Value::Uint(u64::from(*n)))
                .put(pk::THRESHOLD_HOLDER, Value::Bytes(holder.clone())),
        };
        built
            .build()
            .map(Some)
            .map_err(|_| EnvelopeError::BadParams(ParamsError::WrongKind))
    }

    /// Whether the `params` of this kind define the key.
    fn defines(kind: ParamsKind, key: u64) -> bool {
        let keys: &[u64] = match kind {
            ParamsKind::None => &[],
            ParamsKind::Argon2id => &[1, 2, 3, 4],
            ParamsKind::Prf => &[1, 2],
            ParamsKind::Shamir => &[1, 2, 3, 4],
            ParamsKind::Attested => &[1, 2],
            ParamsKind::Threshold => &[1, 2, 3],
        };
        keys.contains(&key)
    }

    /// The `params` of an envelope whose algorithm wants `kind`, from the value under key 16.
    fn from_value(kind: ParamsKind, value: Option<&Value>) -> Result<Params, EnvelopeError> {
        match (kind, value) {
            (ParamsKind::None, None) => Ok(Params::None),
            (ParamsKind::None, Some(_)) => Err(EnvelopeError::FieldForbidden(field::PARAMS)),
            (_, None) => Err(EnvelopeError::FieldRequired(field::PARAMS)),
            (kind, Some(value)) => Params::read(kind, value).map_err(EnvelopeError::BadParams),
        }
    }

    /// Reads a `params` map of `kind`: only the keys of the kind, each of its own type, then the
    /// ranges.
    fn read(kind: ParamsKind, value: &Value) -> Result<Params, ParamsError> {
        let Value::Map(map) = value else {
            return Err(ParamsError::NotAMap);
        };
        for (k, _) in map.iter() {
            match k {
                Value::Uint(n) if Params::defines(kind, *n) => {}
                Value::Uint(n) => return Err(ParamsError::UnknownKey(*n)),
                _ => return Err(ParamsError::Shape(ShapeError::NotAnIntMap)),
            }
        }
        let mut m = IntMap::new(value)?;
        let params = match kind {
            ParamsKind::None => return Err(ParamsError::WrongKind),
            ParamsKind::Argon2id => Params::Argon2id {
                salt: m.req_bytes_n::<16>(pk::ARGON_SALT)?,
                m_kib: narrow(pk::ARGON_M_KIB, m.req_u64(pk::ARGON_M_KIB)?)?,
                t: narrow(pk::ARGON_T, m.req_u64(pk::ARGON_T)?)?,
                p: narrow(pk::ARGON_P, m.req_u64(pk::ARGON_P)?)?,
            },
            ParamsKind::Prf => Params::Prf {
                salt: m.req_bytes_n::<32>(pk::PRF_SALT)?,
                cred_id_hash: m.req_bytes_n::<32>(pk::PRF_CRED)?,
            },
            ParamsKind::Shamir => Params::Shamir {
                x: narrow(pk::SHAMIR_X, m.req_u64(pk::SHAMIR_X)?)?,
                k: narrow(pk::SHAMIR_K, m.req_u64(pk::SHAMIR_K)?)?,
                n: narrow(pk::SHAMIR_N, m.req_u64(pk::SHAMIR_N)?)?,
                split_id: m.req_bytes_n::<4>(pk::SHAMIR_SPLIT)?,
            },
            ParamsKind::Attested => Params::Attested {
                evidence: m.req_bytes(pk::ATTESTED_EVIDENCE)?.to_vec(),
                policy: m.req_bytes(pk::ATTESTED_POLICY)?.to_vec(),
            },
            ParamsKind::Threshold => Params::Threshold {
                k: narrow(pk::THRESHOLD_K, m.req_u64(pk::THRESHOLD_K)?)?,
                n: narrow(pk::THRESHOLD_N, m.req_u64(pk::THRESHOLD_N)?)?,
                holder: m.req_bytes(pk::THRESHOLD_HOLDER)?.to_vec(),
            },
        };
        // Every key was checked above, so nothing is unknown and `crit` (key 0) cannot be there.
        m.finish()?;
        params.validate()?;
        Ok(params)
    }
}

/// A signed envelope, as it is stored in the registry and sent over the wire. Every field is
/// public data (the ciphertext is sealed); the plaintext exists only inside `open`.
///
/// Build one with `wrap_x25519`, `wrap_hybrid` or `seal_with_key`, read one with
/// [`Envelope::from_cbor`]. The fields are public so that a verifier and a copier can read
/// them, and the shape rules of [`rules`] are enforced where it matters: [`Envelope::to_cbor`]
/// writes nothing that [`Envelope::from_cbor`] refuses, and `open` and `verify_signature` refuse
/// a hand-built envelope that breaks them.
///
/// Caller: INT-S15-1 (azul-storage wraps and unwraps), INT-S15-2 (s3d/meta validates stored
/// wraps), S15c1, S15c2, S15d (this crate).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Envelope {
    /// The algorithm.
    pub alg: Alg,
    /// The id of the recipient's public keys (`KeyId::of_bundle`).
    pub rcpt: KeyId,
    /// The ephemeral public key (algorithms 1, 2 and 7; optional for 5 and 6).
    pub epk: Option<[u8; 32]>,
    /// The ML-KEM-768 ciphertext, 1,088 bytes (algorithm 2; optional for 5).
    pub kem_ct: Option<Vec<u8>>,
    /// The XChaCha20-Poly1305 nonce, drawn from the entropy seam.
    pub nonce: [u8; 24],
    /// The ciphertext and its 16-byte tag.
    pub ct: Vec<u8>,
    /// What is wrapped; also part of the associated data.
    pub ctx: Ctx,
    /// The device that wrapped it.
    pub signer: DeviceId,
    /// The signer's signature over the canonical bytes without `sig`.
    pub sig: Signature,
    /// The parameters of algorithms 3 to 7 (`Params::None` for 1 and 2).
    pub params: Params,
    /// The keys a copy must keep as must-understand (rule R18): key 0 of the map. A copy that
    /// drops it would downgrade a marker and change the signed bytes. DRIFT X-DELTA-W1S15B-8:
    /// the brief's `Envelope` has no such field.
    pub crit: Vec<u64>,
    /// The entries of keys this version does not know, in key order, kept so that a copy writes
    /// the same bytes (and the signature still verifies).
    pub unknown: Vec<(Value, Value)>,
}

/// The public header of an envelope under construction: every field but the nonce, the
/// ciphertext, the signature and the copy-only fields. `seal_with_key` makes an envelope of it.
///
/// Seam: S15d (algorithms 3, 4, 5) and S17 (6, 7) build a `Header`, derive their own AEAD key,
/// and call `seal_with_key`; `wrap_x25519` and `wrap_hybrid` build theirs the same way. DRIFT
/// X-DELTA-W1S15B-12: not in the brief's API; X-DELTA-W1S15B-16: what `rcpt` names for the
/// algorithms without a public key is the caller's.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Header {
    /// The algorithm.
    pub alg: Alg,
    /// The id of the recipient's public keys, or whatever the algorithm names the recipient by.
    pub rcpt: KeyId,
    /// The ephemeral public key, when the algorithm has one.
    pub epk: Option<[u8; 32]>,
    /// The ML-KEM-768 ciphertext, when the algorithm has one.
    pub kem_ct: Option<Vec<u8>>,
    /// The parameters of the algorithm.
    pub params: Params,
    /// What is wrapped.
    pub ctx: Ctx,
    /// The wrapping device.
    pub signer: DeviceId,
}

impl Header {
    /// The view the shared code reads.
    fn view(&self) -> HeaderRef<'_> {
        HeaderRef {
            alg: self.alg,
            rcpt: &self.rcpt,
            epk: self.epk.as_ref(),
            kem_ct: self.kem_ct.as_deref(),
            params: &self.params,
            ctx: &self.ctx,
            signer: &self.signer,
        }
    }

    /// The associated data of the AEAD for this header: the canonical CBOR of `v, alg, rcpt,
    /// epk, kem_ct, ctx, signer` (and `params`). The header must follow the field rules of its
    /// algorithm.
    ///
    /// Caller: `seal_with_key` and the construction of each algorithm (this crate), the tests;
    /// S15d reads it through `seal_with_key`.
    pub fn header_ad(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.view().ad()
    }
}

/// The header fields of an envelope or a `Header`, by reference: the one place their rules are
/// checked and their map is written, so the associated data and the full map cannot differ.
struct HeaderRef<'a> {
    alg: Alg,
    rcpt: &'a KeyId,
    epk: Option<&'a [u8; 32]>,
    kem_ct: Option<&'a [u8]>,
    params: &'a Params,
    ctx: &'a Ctx,
    signer: &'a DeviceId,
}

/// Whether a field that `rule` governs may be there (`present`), or must be.
fn presence(rule: Presence, key: u64, present: bool) -> Result<(), EnvelopeError> {
    match (rule, present) {
        (Presence::Required, false) => Err(EnvelopeError::FieldRequired(key)),
        (Presence::Forbidden, true) => Err(EnvelopeError::FieldForbidden(key)),
        _ => Ok(()),
    }
}

/// The error of a map that could not be built: only a hand-built envelope can cause one.
fn map_error(e: MapError) -> EnvelopeError {
    match e {
        MapError::BadCrit => EnvelopeError::BadCrit,
        _ => EnvelopeError::BadUnknown,
    }
}

impl HeaderRef<'_> {
    /// The field rules of the algorithm: presence, the length of `kem_ct`, the kind and the
    /// ranges of `params`.
    fn check(&self) -> Result<(), EnvelopeError> {
        let rule = rules(self.alg);
        presence(rule.epk, field::EPK, self.epk.is_some())?;
        presence(rule.kem_ct, field::KEM_CT, self.kem_ct.is_some())?;
        // A KEM ciphertext belongs to a hybrid, which has the ephemeral X25519 key too.
        if self.kem_ct.is_some() && self.epk.is_none() {
            return Err(EnvelopeError::FieldRequired(field::EPK));
        }
        if let Some(kem_ct) = self.kem_ct {
            if kem_ct.len() != KEM_CT_LEN {
                return Err(EnvelopeError::Shape(ShapeError::WrongLength(field::KEM_CT)));
            }
        }
        match (rule.params, self.params.kind()) {
            (ParamsKind::None, ParamsKind::None) => {}
            (ParamsKind::None, _) => return Err(EnvelopeError::FieldForbidden(field::PARAMS)),
            (_, ParamsKind::None) => return Err(EnvelopeError::FieldRequired(field::PARAMS)),
            (want, have) if want == have => {}
            _ => return Err(EnvelopeError::BadParams(ParamsError::WrongKind)),
        }
        self.params.validate().map_err(EnvelopeError::BadParams)
    }

    /// Puts the header fields into `builder`.
    fn put(&self, builder: IntMapBuilder) -> Result<IntMapBuilder, EnvelopeError> {
        let mut builder = builder
            .put(field::V, Value::Uint(u64::from(Envelope::V)))
            .put(field::ALG, Value::Uint(u64::from(self.alg.number())))
            .put(field::RCPT, Value::Bytes(self.rcpt.as_bytes().to_vec()))
            .put(field::CTX, Value::Text(self.ctx.as_str().to_owned()))
            .put(field::SIGNER, Value::Bytes(self.signer.as_bytes().to_vec()));
        if let Some(epk) = self.epk {
            builder = builder.put(field::EPK, Value::Bytes(epk.to_vec()));
        }
        if let Some(kem_ct) = self.kem_ct {
            builder = builder.put(field::KEM_CT, Value::Bytes(kem_ct.to_vec()));
        }
        if let Some(params) = self.params.to_value()? {
            builder = builder.put(field::PARAMS, params);
        }
        Ok(builder)
    }

    /// The associated data: the canonical CBOR of the header fields alone.
    fn ad(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.check()?;
        let value = self.put(IntMapBuilder::new())?.build().map_err(map_error)?;
        encode(&value, &WIRE_LIMITS).map_err(EnvelopeError::Encode)
    }
}

impl Envelope {
    /// The version: 2. It is the first field of the map, and a reader reads it first, so an
    /// envelope of another version is refused as `UnsupportedVersion` whatever else it holds.
    ///
    /// Caller: the wire codec, the tests.
    pub const V: u8 = 2;

    /// The view the shared code reads.
    fn view(&self) -> HeaderRef<'_> {
        HeaderRef {
            alg: self.alg,
            rcpt: &self.rcpt,
            epk: self.epk.as_ref(),
            kem_ct: self.kem_ct.as_deref(),
            params: &self.params,
            ctx: &self.ctx,
            signer: &self.signer,
        }
    }

    /// The rules of the format that hold whatever made the envelope: the field rules of the
    /// algorithm, the length of the ciphertext, and the unknown entries and `crit` of a copy.
    pub(crate) fn check_rules(&self) -> Result<(), EnvelopeError> {
        self.view().check()?;
        if self.ct.len() < TAG_LEN || self.ct.len() > MAX_PLAINTEXT + TAG_LEN {
            return Err(EnvelopeError::Shape(ShapeError::WrongLength(field::CT)));
        }
        let mut seen = BTreeSet::new();
        for (k, _) in &self.unknown {
            match k {
                Value::Uint(n) if *n != 0 && !field::KNOWN.contains(n) && seen.insert(*n) => {}
                _ => return Err(EnvelopeError::BadUnknown),
            }
        }
        if self.crit.iter().any(|k| !field::KNOWN.contains(k)) {
            return Err(EnvelopeError::BadCrit);
        }
        Ok(())
    }

    /// The map of the envelope, with or without `sig`.
    fn build(&self, with_sig: bool) -> Result<Value, EnvelopeError> {
        self.check_rules()?;
        let mut builder = self
            .view()
            .put(IntMapBuilder::new())?
            .put(field::NONCE, Value::Bytes(self.nonce.to_vec()))
            .put(field::CT, Value::Bytes(self.ct.clone()));
        if with_sig {
            builder = builder.put(field::SIG, Value::Bytes(self.sig.as_bytes().to_vec()));
        }
        for (k, v) in &self.unknown {
            match k {
                Value::Uint(n) => builder = builder.put(*n, v.clone()),
                _ => return Err(EnvelopeError::BadUnknown),
            }
        }
        if !self.crit.is_empty() {
            builder = builder.crit(&self.crit);
        }
        builder.build().map_err(map_error)
    }

    /// The envelope as strict deterministic CBOR: the bytes that are stored, sent and compared.
    /// Nothing is written that [`Envelope::from_cbor`] would refuse: an envelope that breaks
    /// the field rules of its algorithm, or a limit of the format, is an error here.
    ///
    /// Caller: INT-S15-1, INT-S15-2 (the registry stores these bytes untouched), S15c1, S15c2,
    /// S15d.
    pub fn to_cbor(&self) -> Result<Vec<u8>, EnvelopeError> {
        let value = self.build(true)?;
        encode(&value, &WIRE_LIMITS).map_err(EnvelopeError::Encode)
    }

    /// The envelope of these bytes, which must be exactly one envelope in the deterministic
    /// encoding: strict CBOR, `v` 2, a known algorithm, every field of the right type and
    /// length, the field rules of the algorithm, a `ctx` in the grammar, closed `params`; an
    /// unknown key is ignored (and kept in [`Envelope::unknown`]) unless `crit` lists it, and
    /// a listed key this version does not know refuses the envelope.
    ///
    /// For every `Ok(envelope)`, `envelope.to_cbor()` is exactly `bytes`.
    ///
    /// Caller: INT-S15-1, INT-S15-2, S15c1, S15c2, S15d; `check_shape` (the node), the fuzz
    /// target `envelope_decode`.
    pub fn from_cbor(bytes: &[u8]) -> Result<Envelope, EnvelopeError> {
        let decoded = decode_secret(bytes, &WIRE_LIMITS).map_err(EnvelopeError::Cbor)?;
        let envelope = Envelope::read(decoded.as_value())?;
        // A signature covers canonical bytes: an envelope that does not write back to what was
        // read is refused, so the bytes a node stores are the bytes the signer signed.
        if envelope.to_cbor()?.as_slice() != bytes {
            return Err(EnvelopeError::NotCanonical);
        }
        Ok(envelope)
    }

    /// Reads the envelope out of a decoded value.
    fn read(value: &Value) -> Result<Envelope, EnvelopeError> {
        let mut m = IntMap::new(value).map_err(EnvelopeError::Shape)?;
        // The version first: a reader that does not know the version knows nothing else.
        let v = m.req_u64(field::V).map_err(EnvelopeError::Shape)?;
        if v != u64::from(Envelope::V) {
            return Err(EnvelopeError::UnsupportedVersion(v));
        }
        let alg_number = m.req_u64(field::ALG).map_err(EnvelopeError::Shape)?;
        let alg = Alg::from_number(alg_number).ok_or(EnvelopeError::UnknownAlg(alg_number))?;
        let rcpt = m
            .req_bytes_n::<32>(field::RCPT)
            .map_err(EnvelopeError::Shape)?;
        let epk = m
            .opt_bytes_n::<32>(field::EPK)
            .map_err(EnvelopeError::Shape)?;
        let kem_ct = m.opt_bytes(field::KEM_CT).map_err(EnvelopeError::Shape)?;
        let nonce = m
            .req_bytes_n::<24>(field::NONCE)
            .map_err(EnvelopeError::Shape)?;
        let ct = m.req_bytes(field::CT).map_err(EnvelopeError::Shape)?;
        let ctx = m.req_text(field::CTX).map_err(EnvelopeError::Shape)?;
        let signer = m
            .req_bytes_n::<16>(field::SIGNER)
            .map_err(EnvelopeError::Shape)?;
        let sig = m
            .req_bytes_n::<64>(field::SIG)
            .map_err(EnvelopeError::Shape)?;
        let params = m.opt_value(field::PARAMS).map_err(EnvelopeError::Shape)?;
        let crit = m.crit_keys().map_err(EnvelopeError::Shape)?;
        let unknown: Vec<(Value, Value)> = m
            .finish()
            .map_err(EnvelopeError::Shape)?
            .into_iter()
            .map(|(k, v)| (k.clone(), v.clone()))
            .collect();
        let ctx = Ctx::parse(ctx).map_err(EnvelopeError::BadCtx)?;
        let params = Params::from_value(rules(alg).params, params)?;
        let envelope = Envelope {
            alg,
            rcpt: KeyId::from_bytes(rcpt),
            epk,
            kem_ct: kem_ct.map(<[u8]>::to_vec),
            nonce,
            ct: ct.to_vec(),
            ctx,
            signer: DeviceId::from_bytes(signer),
            sig: Signature::from_bytes(sig),
            params,
            crit,
            unknown,
        };
        envelope.check_rules()?;
        Ok(envelope)
    }

    /// The bytes the signature covers: the canonical CBOR of the envelope WITHOUT `sig` and
    /// with everything else, the ciphertext, the nonce, the unknown fields and `crit` included.
    /// The signature itself is made over `azlin-envelope-v2 0x00` followed by these bytes
    /// (`azlin_keys::sign::signing_input`).
    ///
    /// Caller: `Envelope::sign`, `verify_signature` (this crate), the tests.
    pub fn signing_input(&self) -> Result<Vec<u8>, EnvelopeError> {
        let value = self.build(false)?;
        encode(&value, &WIRE_LIMITS).map_err(EnvelopeError::Encode)
    }

    /// The associated data of the AEAD for this envelope: the canonical CBOR of its header
    /// fields, the same bytes [`Header::header_ad`] gives for the header it was made from.
    ///
    /// Caller: `open`, `open_with_key` (this crate), the tests.
    pub fn header_ad(&self) -> Result<Vec<u8>, EnvelopeError> {
        self.view().ad()
    }

    /// Signs the envelope in place: `sig` becomes the signature of `key` over
    /// [`Envelope::signing_input`] under the domain `azlin-envelope-v2`. A lane that copies an
    /// envelope and changes it signs the copy with this.
    ///
    /// Caller: `wrap_x25519`, `wrap_hybrid`, `seal_with_key` (this crate); a lane that re-signs
    /// a copy (S15c2).
    pub fn sign(&mut self, key: &SignKeypair) -> Result<(), EnvelopeError> {
        let message = self.signing_input()?;
        self.sig = sign::sign(&ENVELOPE_V2, &message, key);
        Ok(())
    }
}

/// The associated data of a header WITHOUT the field rules of its algorithm, for the tests that
/// change one field (the algorithm number among them) and show that the AEAD refuses the change.
#[cfg(test)]
impl Header {
    pub(crate) fn header_ad_unchecked_for_test(&self) -> Vec<u8> {
        let value = self
            .view()
            .put(IntMapBuilder::new())
            .expect("a header that can be written")
            .build()
            .expect("a map");
        encode(&value, &WIRE_LIMITS).expect("within the limits")
    }
}
