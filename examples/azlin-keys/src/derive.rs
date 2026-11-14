//! The registry of every BLAKE3 `derive_key` context of the keys family: one key, one purpose
//! (planning/architecture.md section 5.5: "BLAKE3 `derive_key` with separate contexts gives
//! `K_seal` ... and `K_chunk`"; build unit S15, lane W1-S15a).
//!
//! A [`Context`] has a crate-private constructor, so a context exists only because it is written
//! in the `contexts!` table below; the table makes the constants and [`CONTEXTS`] one list that
//! cannot drift apart. The strings follow `azlin 2026-11 <purpose> v<N>` (the two
//! strings the plan itself gives, `azlin chunk id v1` and `azlin chunk key v1`, belong to the
//! content family: azlin-content keeps its own table and tests that it is disjoint from this
//! one). A context is never edited: a changed purpose is a new context with the next `v<N>`.
//! The known answers of every entry are pinned in `tests/vectors/derive_kat.json`.
//!
//! Every entry documents the section of the plan it serves and the lane that uses it.
//!
//! ```compile_fail
//! // A context cannot be made outside the crate: there is no way to derive under a purpose that
//! // is not in the table.
//! use azlin_keys::derive::Context;
//! let _made_up = Context::new("made up", "azlin made up v1");
//! ```

use zeroize::Zeroizing;

use crate::secret::Secret;

/// One BLAKE3 `derive_key` context string of the registry, with the name of its constant.
///
/// Caller: `derive`, `derive_public`, S15b/S15c1/S15d/S15e (the keys they derive), S14b's
/// disjointness test (`as_str`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Context {
    name: &'static str,
    text: &'static str,
}

impl Context {
    /// A context. Crate-private: a context is an entry of the table below, nothing else.
    pub(crate) const fn new(name: &'static str, text: &'static str) -> Context {
        Context { name, text }
    }

    /// The context string handed to BLAKE3.
    pub const fn as_str(&self) -> &'static str {
        self.text
    }

    /// The name of the constant (`K_SEAL`), for tables and known-answer files.
    pub const fn name(&self) -> &'static str {
        self.name
    }
}

/// Declares the contexts: one constant each, and `CONTEXTS` listing every one in this order.
macro_rules! contexts {
    ($( $(#[$meta:meta])* $name:ident = $text:literal; )+) => {
        $(
            $(#[$meta])*
            pub const $name: Context = Context::new(stringify!($name), $text);
        )+

        /// Every derive context of the keys family, in the order of the table. Appending an entry
        /// changes the known-answer file `tests/vectors/derive_kat.json`, which then has to be
        /// regenerated with `UPDATE_VECTORS=1` and reviewed.
        ///
        /// Caller: S14b's test `c20_contexts_are_disjoint_from_azlin_keys`, INT-WIRE (the
        /// contract fixture `c4_derive_kat.json`).
        pub const CONTEXTS: &[Context] = &[$( $name ),+];
    };
}

contexts! {
    /// The id of a recipient's public key bundle (`rcpt` of an envelope, "recipient key id" of a
    /// `reg_wrap` row). Cites planning/architecture.md section 5.5 (envelope v2); lane W1-S15a
    /// (`KeyId::of_bundle`, DRIFT X-DELTA-W1S15A-1).
    ///
    /// Caller: `KeyId::of_bundle`.
    KEY_ID = "azlin key id v1";

    /// The id a signed `space_create` or `space_rekey` chain entry commits a space epoch's key
    /// under. Cites planning/architecture.md section 5.5 ("Epochs must be authorized"); lane
    /// W1-S15c1 (`SpaceKey::key_id`).
    ///
    /// Caller: S15c1 `SpaceKey::key_id`, committed by S16b's chain entries.
    SPACE_KEY_ID = "azlin 2026-11 space key id v1";

    /// The id of a group key epoch. Cites planning/architecture.md section 5.5 (group key, per
    /// epoch); lane W1-S15c1 (`GroupKey::id`).
    ///
    /// Caller: S15c1 `GroupKey::id`.
    GROUP_KEY_ID = "azlin 2026-11 group key id v1";

    /// `K_seal` of a space epoch: the key of the repository, the pack trailers and the pointers.
    /// Cites planning/architecture.md section 5.5 (space key, `K_seal`); lane W1-S15c1
    /// (`SpaceKey::k_seal`).
    ///
    /// Caller: S15c1 `SpaceKey::k_seal`, used by S14c (pack trailers) and S14d (pointers).
    K_SEAL = "azlin 2026-11 space k_seal v1";

    /// `K_chunk` of a space's dedup epoch: the key of chunk ids and chunk keys. Cites
    /// planning/architecture.md section 5.5 and 5.7 (`K_chunk`); lane W1-S15c1
    /// (`SpaceKey::k_chunk`).
    ///
    /// Caller: S15c1 `SpaceKey::k_chunk`, taken by S14b's `KChunk`.
    K_CHUNK = "azlin 2026-11 space k_chunk v1";

    /// The key that seals epoch n under epoch n+1 (the epoch chain). Cites
    /// planning/architecture.md section 5.5 ("Rotation ... plus the chain wrap"); lane W1-S15c1
    /// (`chain_wrap`).
    ///
    /// Caller: S15c1 `chain_wrap` and `chain_open`.
    CHAIN_WRAP = "azlin 2026-11 epoch chain wrap v1";

    /// The key of an envelope of algorithm 1 (X25519 sealed box), from the shared secret, the
    /// ephemeral key and the recipient key. Cites planning/architecture.md section 5.5 (envelope
    /// v2, `alg` 1); lane W1-S15b.
    ///
    /// Caller: S15b `wrap_x25519` and `open`.
    ENVELOPE_ALG1_KEY = "azlin 2026-11 envelope v2 alg1 key v1";

    /// The key of an envelope of algorithm 2 (X25519 + ML-KEM-768), from the transcript-binding
    /// combiner. Cites planning/architecture.md section 5.5 (envelope v2, `alg` 2); lane
    /// W1-S15b.
    ///
    /// Caller: S15b `combiner::combine`.
    ENVELOPE_ALG2_KEY = "azlin 2026-11 envelope v2 alg2 key v1";

    /// The key of an envelope of algorithm 3, from the Argon2id root of the recovery code. Cites
    /// planning/architecture.md section 5.5 (Recovery, `alg` 3); lane W1-S15d.
    ///
    /// Caller: S15d `wrap_with_code` and `unwrap_with_code`.
    RECOVERY_WRAP = "azlin 2026-11 recovery code wrap v1";

    /// The check value that detects a wrong recombination of a recovery code. Cites
    /// planning/architecture.md section 5.5 (Recovery: the code, the contacts' shares); lane
    /// W1-S15d (`RecoveryCode::check_value`).
    ///
    /// Caller: S15d `RecoveryCode::check_value`.
    RECOVERY_CHECK = "azlin 2026-11 recovery code check v1";

    /// The key of an envelope of algorithm 4, from the passkey's PRF output. Cites
    /// planning/architecture.md section 5.5 (Recovery: the passkey PRF, `alg` 4); lane W1-S15d.
    ///
    /// Caller: S15d `wrap_with_prf` and `unwrap_with_prf`.
    PRF_WRAP = "azlin 2026-11 passkey prf wrap v1";

    /// The key of a bound secret: the ECIES-style wrap of a device key under the hardware P-256
    /// binding key. Cites planning/architecture.md section 5.5 (Device keys) and section 7.4;
    /// lane W1-S15e (`bind`, `unbind`).
    ///
    /// Caller: S15e `bind` and `unbind`.
    BINDING_WRAP = "azlin 2026-11 binding wrap v1";

    /// The id of a hardware binding public key. Cites planning/architecture.md section 5.5
    /// (Device keys) and section 7.4; lane W1-S15e (`BoundSecret::binding_key_id`).
    ///
    /// Caller: S15e `BoundSecret::binding_key_id`.
    BINDING_KEY_ID = "azlin 2026-11 binding key id v1";
}

/// The key derived from `key` under `ctx`: BLAKE3 `derive_key`, one key for one purpose. The
/// temporary array BLAKE3 returns is wiped; the result lives only in the returned [`Secret`].
///
/// Caller: S15c1 (`K_seal`, `K_chunk` of a space epoch), S15b/S15d/S15e (wrapping keys).
pub fn derive(ctx: &Context, key: &Secret<32>) -> Secret<32> {
    let derived = Zeroizing::new(blake3::derive_key(ctx.as_str(), key.expose()));
    Secret::from_bytes(*derived)
}

/// The 32 bytes BLAKE3 `derive_key` gives for public `material` under `ctx`: an id or a check
/// value, never a key. A derivation from secret material is fine as long as the output is a
/// one-way commitment that is meant to be published (a key id); a derivation whose output is used
/// as a key is [`derive`].
///
/// Caller: `KeyId::of_bundle`, S15c1 (`SpaceKey::key_id`, `GroupKey::id`), S15e (the binding key
/// id), S15d (`check_value`).
pub fn derive_public(ctx: &Context, material: &[u8]) -> [u8; 32] {
    blake3::derive_key(ctx.as_str(), material)
}
