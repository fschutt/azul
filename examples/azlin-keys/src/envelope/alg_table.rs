//! The algorithms of envelope v2 and the field rules of each, as DATA
//! (planning/architecture.md section 5.5, "Envelope v2 (every wrap)", the `alg` row; build unit
//! S15, lane W1-S15b).
//!
//! The table [`ALG_TABLE`] repeats the plan's row word for word, so that the doc-table test
//! (INT-PLAN-TABLES, and `c12_alg_table_matches_the_plan_text` here) can compare it with the
//! plan's text. [`rules`] says, for each algorithm, which of the optional fields `epk`, `kem_ct`
//! and `params` must be present, must be absent or may be either: the ONE place the shape of
//! an envelope per algorithm is written down. `Envelope::from_cbor`, `Envelope::to_cbor`,
//! `check_shape` and `open` all enforce it, so a node that stores wraps it can never open
//! refuses the same shapes a client refuses.
//!
//! Algorithms 1 and 2 are built here, algorithms 3 to 5 get their AEAD layer here and their
//! constructions in lane W1-S15d, algorithms 6 and 7 are SHAPE-ONLY here (their cryptography is
//! S17's).

/// The algorithm of an envelope: the `alg` field, a number from 1 to 7 and nothing else. An
/// unknown number is refused (`UnknownAlg`), never skipped, so an envelope of a newer
/// algorithm is never mistaken for an older one.
///
/// Caller: every lane that wraps or opens a key (INT-S15-1, S15c1, S15c2, S15d, S17).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Alg {
    /// 1: X25519 sealed box: an ephemeral X25519 key, BLAKE3 `derive_key`, XChaCha20-Poly1305.
    X25519SealedBox = 1,
    /// 2: X25519 + ML-KEM-768 hybrid: both halves feed one transcript-binding combiner.
    X25519MlKem768 = 2,
    /// 3: Argon2id passphrase (the recovery code). Built in lane W1-S15d.
    Argon2id = 3,
    /// 4: the PRF output of a passkey. Built in lane W1-S15d.
    Prf = 4,
    /// 5: a Shamir share. Built in lane W1-S15d.
    Shamir = 5,
    /// 6: sealed to an attested key (`evidence`, `policy`). Shape only here: S17.
    Attested = 6,
    /// 7: threshold escrow. Shape only here: S17.
    Threshold = 7,
}

impl Alg {
    /// All seven, in the order of the plan's table.
    pub const ALL: [Alg; 7] = [
        Alg::X25519SealedBox,
        Alg::X25519MlKem768,
        Alg::Argon2id,
        Alg::Prf,
        Alg::Shamir,
        Alg::Attested,
        Alg::Threshold,
    ];

    /// The number of the algorithm, as written in the `alg` field.
    pub const fn number(self) -> u8 {
        self as u8
    }

    /// The algorithm with this number; `None` for every other number (0, 8, 255 and every
    /// number that does not fit a byte).
    pub const fn from_number(n: u64) -> Option<Alg> {
        match n {
            1 => Some(Alg::X25519SealedBox),
            2 => Some(Alg::X25519MlKem768),
            3 => Some(Alg::Argon2id),
            4 => Some(Alg::Prf),
            5 => Some(Alg::Shamir),
            6 => Some(Alg::Attested),
            7 => Some(Alg::Threshold),
            _ => None,
        }
    }

    /// The plan's words for the algorithm (the text of its row in [`ALG_TABLE`]).
    pub const fn title(self) -> &'static str {
        match self {
            Alg::X25519SealedBox => "X25519 sealed box",
            Alg::X25519MlKem768 => "X25519 + ML-KEM-768 hybrid",
            Alg::Argon2id => "Argon2id passphrase",
            Alg::Prf => "PRF",
            Alg::Shamir => "Shamir share",
            Alg::Attested => "sealed to an attested key (`evidence`, `policy`)",
            Alg::Threshold => "threshold escrow",
        }
    }
}

/// The `alg` row of the plan's envelope table (planning/architecture.md section 5.5: "1 X25519
/// sealed box; 2 X25519 + ML-KEM-768 hybrid; 3 Argon2id passphrase; 4 PRF; 5 Shamir share; 6
/// sealed to an attested key (`evidence`, `policy`); 7 threshold escrow"), one `(number, words)`
/// pair per algorithm, as data for the doc-table test.
///
/// Caller: INT-PLAN-TABLES (the comparison with the plan's text), the test `c12_*` of this lane.
pub const ALG_TABLE: &[(u8, &str)] = &[
    (1, "X25519 sealed box"),
    (2, "X25519 + ML-KEM-768 hybrid"),
    (3, "Argon2id passphrase"),
    (4, "PRF"),
    (5, "Shamir share"),
    (6, "sealed to an attested key (`evidence`, `policy`)"),
    (7, "threshold escrow"),
];

/// Whether an optional field of an envelope must be there, must not be there, or may be either.
///
/// Caller: [`rules`], the shape checks of `wire.rs`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Presence {
    /// The field must be present.
    Required,
    /// The field must be absent.
    Forbidden,
    /// The field may be present (its length is still checked when it is).
    Optional,
}

/// Which `params` map an algorithm carries (DRIFT X-DELTA-W1S15B-1: the plan's envelope table
/// has no field for the parameters of algorithms 3 to 7; P-2 puts them in one map under key 16).
///
/// Caller: [`rules`], `Params::kind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum ParamsKind {
    /// No `params` (algorithms 1 and 2).
    None,
    /// Algorithm 3: `{salt, m_kib, t, p}`.
    Argon2id,
    /// Algorithm 4: `{salt, cred_id_hash}`.
    Prf,
    /// Algorithm 5: `{x, k, n, split_id}`.
    Shamir,
    /// Algorithm 6: `{evidence, policy}`.
    Attested,
    /// Algorithm 7: `{k, n, holder}`.
    Threshold,
}

/// The field rules of one algorithm: the presence of `epk` and `kem_ct`, and the kind of
/// `params`. `v`, `alg`, `rcpt`, `nonce`, `ct`, `ctx`, `signer` and `sig` are required for every
/// algorithm.
///
/// Caller: [`rules`], the shape checks of `wire.rs`, the data tests `c6` and `c12`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct FieldRules {
    /// The ephemeral public key (32 bytes).
    pub epk: Presence,
    /// The ML-KEM-768 ciphertext (1,088 bytes).
    pub kem_ct: Presence,
    /// The kind of `params`: `ParamsKind::None` means the field must be absent, every other
    /// kind means it must be present and of that kind.
    pub params: ParamsKind,
}

/// The field rules of `alg` (the brief's PER-ALG FIELD RULES). Algorithm 1: `epk` required,
/// `kem_ct` forbidden. Algorithm 2: both required. Algorithms 3 and 4: `epk` and `kem_ct`
/// forbidden, `params` required. Algorithm 5: `params {x, k, n, split_id}` required, `epk` and
/// `kem_ct` OPTIONAL (a share wrapped to a contact is sealed like algorithm 1 or 2: W1-S15d's
/// reading; a `kem_ct` needs its `epk`). Algorithm 6: `params {evidence, policy}`, `epk`
/// optional. Algorithm 7: `params {k, n, holder}`, `epk` required (it carries the Ristretto point
/// of the hashed ElGamal wrap, S17f's reading). `kem_ct` is forbidden for algorithms 6 and 7.
///
/// DRIFT X-DELTA-W1S15B-11: the brief gives no `epk` rule for algorithms 6 and 7 and forbids
/// `epk` and `kem_ct` for algorithm 5, which W1-S15d's sealed shares contradict; this is the
/// one place to change it.
///
/// Caller: `Envelope::from_cbor`, `Envelope::to_cbor`, `check_shape`, `open`, INT-PLAN-TABLES.
pub const fn rules(alg: Alg) -> FieldRules {
    match alg {
        Alg::X25519SealedBox => FieldRules {
            epk: Presence::Required,
            kem_ct: Presence::Forbidden,
            params: ParamsKind::None,
        },
        Alg::X25519MlKem768 => FieldRules {
            epk: Presence::Required,
            kem_ct: Presence::Required,
            params: ParamsKind::None,
        },
        Alg::Argon2id => FieldRules {
            epk: Presence::Forbidden,
            kem_ct: Presence::Forbidden,
            params: ParamsKind::Argon2id,
        },
        Alg::Prf => FieldRules {
            epk: Presence::Forbidden,
            kem_ct: Presence::Forbidden,
            params: ParamsKind::Prf,
        },
        Alg::Shamir => FieldRules {
            epk: Presence::Optional,
            kem_ct: Presence::Optional,
            params: ParamsKind::Shamir,
        },
        Alg::Attested => FieldRules {
            epk: Presence::Optional,
            kem_ct: Presence::Forbidden,
            params: ParamsKind::Attested,
        },
        Alg::Threshold => FieldRules {
            epk: Presence::Required,
            kem_ct: Presence::Forbidden,
            params: ParamsKind::Threshold,
        },
    }
}
