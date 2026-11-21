//! The two key derivations of envelope v2: the one of algorithm 1 and the hybrid COMBINER of
//! algorithm 2 (planning/architecture.md section 5.5: "2 X25519 + ML-KEM-768 hybrid"; build unit
//! S15, lane W1-S15b).
//!
//! A badly combined hybrid is weaker than either half, so the combination lives in ONE function,
//! [`combine`], and nowhere else. It is the transcript-binding construction of the brief (P-1;
//! DRIFT X-DELTA-W1S15B-4, the owner decides at the external review whether X-Wing's SHA3-256
//! construction replaces it):
//!
//! ```text
//! key = BLAKE3 derive_key("azlin 2026-11 envelope v2 alg2 key v1",
//!                         ss_mlkem || ss_x25519 || kem_ct || epk || rcpt_kx || BLAKE3(pq))
//! ```
//!
//! The key depends on BOTH shared secrets, and on every public value the two key agreements were
//! made over: the ML-KEM ciphertext, the ephemeral X25519 key, the recipient's X25519 key and the
//! hash of the recipient's ML-KEM encapsulation key. Breaking one half of the hybrid does not give
//! the key, and an attacker who changes any of the public values gets another key. Algorithm 1
//! derives its key the same way, from `shared || epk || rcpt_kx` under its own context
//! ([`alg1_key`]), as today's member wrap (`crypto/keys.rs`) does.
//!
//! The contexts are entries of the registry in `derive.rs` (`ENVELOPE_ALG1_KEY`,
//! `ENVELOPE_ALG2_KEY`), which differ from every string of the old azul-storage keys.
//!
//! The BLAKE3 hasher that sees the shared secrets, and the hash that is the key, are wiped as
//! soon as the key sits in its `Secret` (DRIFT X-DELTA-W1S15A-7: DEP-2 set blake3's `zeroize`
//! feature for azlin-keys; `kdf` is the one place the secrets are hashed).

use zeroize::Zeroize;

use crate::derive::{Context, ENVELOPE_ALG1_KEY, ENVELOPE_ALG2_KEY};
use crate::secret::Secret;

/// The public values the hybrid key is bound to.
///
/// Caller: `combine` (the hybrid wrap and open, this crate), the tests of the combiner.
#[derive(Clone, Copy, Debug)]
pub struct Transcript<'a> {
    /// The ML-KEM-768 ciphertext of the envelope (`kem_ct`), 1,088 bytes.
    pub kem_ct: &'a [u8],
    /// The ephemeral X25519 public key of the envelope (`epk`).
    pub epk: &'a [u8; 32],
    /// The recipient's X25519 public key.
    pub rcpt_kx: &'a [u8; 32],
    /// The recipient's ML-KEM-768 encapsulation key (`pq`), 1,184 bytes; its BLAKE3 hash is
    /// bound, not the key.
    pub pq: &'a [u8],
}

/// The AEAD key of an envelope of algorithm 2: BLAKE3 `derive_key` under the context
/// `azlin 2026-11 envelope v2 alg2 key v1` over `ss_mlkem || ss_x25519 || kem_ct || epk ||
/// rcpt_kx || BLAKE3(pq)`. The ONE place the two halves are combined.
///
/// Caller: `wrap_hybrid` and the hybrid open (this crate), the tests of check C10 (the key
/// changes when either half does).
pub fn combine(
    ss_mlkem: &Secret<32>,
    ss_x25519: &Secret<32>,
    transcript: &Transcript<'_>,
) -> Secret<32> {
    let pq_hash = blake3::hash(transcript.pq);
    let parts: [&[u8]; 6] = [
        ss_mlkem.expose(),
        ss_x25519.expose(),
        transcript.kem_ct,
        transcript.epk,
        transcript.rcpt_kx,
        pq_hash.as_bytes(),
    ];
    kdf(&ENVELOPE_ALG2_KEY, &parts)
}

/// The public values the key of algorithm 1 is bound to, with the shared secret.
///
/// Caller: `alg1_key` (the sealed-box wrap and open, this crate), the tests.
#[derive(Clone, Copy, Debug)]
pub struct Alg1Input<'a> {
    /// The X25519 shared secret of the ephemeral key and the recipient's key.
    pub shared: &'a Secret<32>,
    /// The ephemeral X25519 public key of the envelope (`epk`).
    pub epk: &'a [u8; 32],
    /// The recipient's X25519 public key.
    pub rcpt_kx: &'a [u8; 32],
}

/// The AEAD key of an envelope of algorithm 1: BLAKE3 `derive_key` under the context
/// `azlin 2026-11 envelope v2 alg1 key v1` over `shared || epk || rcpt_kx`.
///
/// Caller: `wrap_x25519` and the sealed-box open (this crate), the tests.
pub fn alg1_key(input: &Alg1Input<'_>) -> Secret<32> {
    let parts: [&[u8]; 3] = [input.shared.expose(), input.epk, input.rcpt_kx];
    kdf(&ENVELOPE_ALG1_KEY, &parts)
}

/// BLAKE3 `derive_key` over the concatenation of `parts` under `context`. The hasher (it has seen
/// the shared secrets) and the hash are wiped; the key lives on only in the `Secret` returned.
fn kdf(context: &Context, parts: &[&[u8]]) -> Secret<32> {
    let mut hasher = blake3::Hasher::new_derive_key(context.as_str());
    for part in parts {
        hasher.update(part);
    }
    let mut hash = hasher.finalize();
    let key = Secret::from_bytes(*hash.as_bytes());
    hasher.zeroize();
    hash.zeroize();
    key
}
