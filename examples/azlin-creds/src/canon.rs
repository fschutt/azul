//! The bytes that are signed (planning/architecture.md section 2.5 "Signed structures, each with its own
//! domain string"; section 5.1 principle 6 "deterministic CBOR (RFC 8949 section 4.2), unknown fields
//! ignored, must-understand fields listed in `crit`"; COMMON-W1R R4 and R6).
//!
//! THE SIGNING RULE: a signature is made over `domain || 0x00 || bytes` (azlin-keys' `signing_input`,
//! never framed here a second time), and the bytes are the ONE canonical encoding of the structure,
//! written and read with `azlin-cbor`. Four structures, four domains:
//!
//! | domain | structure | signed by |
//! |---|---|---|
//! | [`CREDENTIAL_V2`] | the claims of a credential | the metadata group's credential key |
//! | [`MEMBER_ASSERT_V1`] | the claims of a member assertion | the metadata group's credential key |
//! | [`ENTITLEMENT_V2`] | the claims of an entitlement | the payments key |
//! | [`CREDENTIAL_CERT_V1`] | the body of a certificate | the metadata group's identity key |
//!
//! `azlin-credential-cert-v1` is new (DRIFT D-11): section 2.5's list has no certificate. Every domain
//! is a constant ([`ALL_DOMAINS`] lists them, and a test runs `Domain::is_wellformed` over the list).
//!
//! The wire form of each structure is a CBOR map with unsigned-integer keys (the keys are constants of
//! the module that owns the structure). A signed structure keeps the EXACT bytes the signature covers
//! (the payload): a reader ignores a key it does not know and refuses one that `crit` lists, and the
//! signature still verifies because it is checked over the received bytes, never over a re-encoding.
//!
//! Caller: every encoder and decoder of this crate; the golden vectors under `tests/vectors/`.

use core::fmt;

use azlin_cbor::{encode, IntMap, IntMapBuilder, Limits, Value};
use azlin_regbase::{signing_input, Domain, LeaderTime};

use crate::error::{CanonicalError, WireError};

/// The domain of a credential's claims: `azlin-credential-v2` (planning/architecture.md section 2.5).
pub const CREDENTIAL_V2: Domain = Domain::new("azlin-credential-v2");

/// The domain of a member assertion's claims: `azlin-member-assert-v1` (section 2.5).
pub const MEMBER_ASSERT_V1: Domain = Domain::new("azlin-member-assert-v1");

/// The domain of an entitlement's claims: `azlin-entitlement-v2` (section 2.5).
pub const ENTITLEMENT_V2: Domain = Domain::new("azlin-entitlement-v2");

/// The domain of a certificate's body: `azlin-credential-cert-v1`. NEW (DRIFT D-11): section 2.5's
/// list names no certificate, so S5b registers it as `Proposed` (W1/CONFLICTS.md C-12).
pub const CREDENTIAL_CERT_V1: Domain = Domain::new("azlin-credential-cert-v1");

/// The four signature domains this crate defines. It defines no other.
///
/// Caller: the test that runs `Domain::is_wellformed` over the list and checks it has no duplicate;
/// INT-WIRE (the comparison with azlin-contract's `SIGNED_DOMAINS`).
pub const ALL_DOMAINS: &[Domain] = &[
    CREDENTIAL_V2,
    MEMBER_ASSERT_V1,
    ENTITLEMENT_V2,
    CREDENTIAL_CERT_V1,
];

/// A structure that has canonical bytes and a signature domain: the seam between the typed values and
/// the codec (the brief's "codec seam").
///
/// Caller: `signing_preimage`, the issuers, `certify`, `sign_entitlement`, the verifiers.
pub trait Canonical {
    /// The domain a signature over [`Canonical::canonical_bytes`] is made under.
    const DOMAIN: Domain;

    /// The bytes that are signed: the one deterministic encoding of the value, or why there is none.
    fn canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError>;
}

/// The domain a value of type `T` is signed and verified under. A function of `T` alone: every signing
/// and every verifying site asks it, so the two sides can never disagree.
pub(crate) fn domain_of<T: Canonical>() -> Domain {
    if azlin_seams::mutant!("creds_shared_domain") {
        return CREDENTIAL_V2;
    }
    T::DOMAIN
}

/// The preimage `domain || 0x00 || payload` of already encoded bytes of a `T`.
pub(crate) fn preimage_of<T: Canonical>(payload: &[u8]) -> Vec<u8> {
    signing_input(&domain_of::<T>(), payload)
}

/// The bytes a signature over `value` covers: `domain || 0x00 || canonical bytes`.
///
/// Caller: the tests that pin the signing input; S9f (a holder that signs on another machine is handed
/// this preimage's parts through `Signer::sign`, which frames them itself).
pub fn signing_preimage<T: Canonical>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let payload = value.canonical_bytes()?;
    Ok(preimage_of::<T>(&payload))
}

/// The largest encoded credential, assertion, entitlement or certificate: the limit stated up front
/// (planning/architecture.md section 1.4 rule 4).
pub(crate) const WIRE_MAX: u32 = 8 * 1024;

/// The limits of a signed structure as it travels: a map of a few keys around one payload.
pub(crate) const OUTER_LIMITS: Limits = Limits {
    max_input: WIRE_MAX,
    max_depth: 4,
    max_items: 16,
    max_total_items: 64,
    max_text: WIRE_MAX,
};

/// The limits of a payload (the claims): a map whose largest list is the 256 groups of an assertion.
pub(crate) const PAYLOAD_LIMITS: Limits = Limits {
    max_input: WIRE_MAX,
    max_depth: 4,
    max_items: 512,
    max_total_items: 2048,
    max_text: 256,
};

/// How many leading bytes a `Debug` of an id prints.
const SHORT_LEN: usize = 4;

/// `Debug` of the first four bytes of an id, a key or a hash and `..`: never the whole value (QUALITY
/// rule 26).
pub(crate) struct Short<'a>(pub(crate) &'a [u8]);

impl fmt::Debug for Short<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0.iter().take(SHORT_LEN) {
            write!(f, "{byte:02x}")?;
        }
        f.write_str("..")
    }
}

/// A byte string value.
pub(crate) fn bytes_of(bytes: &[u8]) -> Value {
    Value::Bytes(bytes.to_vec())
}

/// An unsigned integer value.
pub(crate) fn uint_of(number: u64) -> Value {
    Value::Uint(number)
}

/// A text value.
pub(crate) fn text_of(text: &str) -> Value {
    Value::Text(text.to_owned())
}

/// The map of a builder, or why it cannot be built.
pub(crate) fn build(builder: IntMapBuilder) -> Result<Value, CanonicalError> {
    builder.build().map_err(|_| CanonicalError::Map)
}

/// The canonical bytes of a value under the limits of its structure.
pub(crate) fn write(value: &Value, limits: &Limits) -> Result<Vec<u8>, CanonicalError> {
    encode(value, limits).map_err(|_| CanonicalError::TooLarge)
}

/// A required unsigned number that must fit a `u16`.
pub(crate) fn read_u16(map: &mut IntMap<'_>, key: u64) -> Result<u16, WireError> {
    u16::try_from(map.req_u64(key)?).map_err(|_| WireError::OutOfRange(key))
}

/// A required unsigned number that must fit a `u32`.
pub(crate) fn read_u32(map: &mut IntMap<'_>, key: u64) -> Result<u32, WireError> {
    u32::try_from(map.req_u64(key)?).map_err(|_| WireError::OutOfRange(key))
}

/// A required leader stamp (whole seconds).
pub(crate) fn read_time(map: &mut IntMap<'_>, key: u64) -> Result<LeaderTime, WireError> {
    Ok(LeaderTime::from_secs(map.req_u64(key)?))
}

/// The end of the reading of a CLOSED schema: no `crit` list, no key nobody asked for. (An open
/// schema calls `IntMap::finish` and ignores what it returns.)
pub(crate) fn finish_closed(map: IntMap<'_>) -> Result<(), WireError> {
    if !map.crit_keys()?.is_empty() {
        return Err(WireError::UnknownField(0));
    }
    let unknown = map.finish()?;
    match unknown.first() {
        None => Ok(()),
        Some((Value::Uint(key), _)) => Err(WireError::UnknownField(*key)),
        Some(_) => Err(WireError::Malformed),
    }
}

/// An id of 16 bytes from a value of a list.
pub(crate) fn id16_of(value: &Value) -> Result<[u8; 16], WireError> {
    match value {
        Value::Bytes(bytes) => {
            <[u8; 16]>::try_from(bytes.as_slice()).map_err(|_| WireError::Malformed)
        }
        _ => Err(WireError::Malformed),
    }
}
