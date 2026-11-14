//! Credential v2: the claims, the certificate of the key that signed them, and the signature
//! (planning/architecture.md section 5.11 "Credential v2 (`azlin-credential-v2`, signed by the metadata
//! group's credential key)"; section 2.4 "Credentials are verifiable on every node").
//!
//! A [`Credential`] keeps the EXACT bytes its signature covers (the payload). A reader that does not know
//! a key of the claims ignores it, a reader that is told by `crit` to understand it refuses, and in both
//! cases the signature is checked over the received bytes (never over a re-encoding of the parsed
//! claims), so an issuer of a newer version can add a field without breaking an older verifier. The
//! verifier reads the claims it returns FROM those bytes, never from a copy.
//!
//! The wire form is a map: key 1 the payload (a byte string: the canonical claims), key 2 the
//! certificate (a map, see `cert.rs`), key 3 the signature.
//!
//! [`VerifiedCredential`] is a witness: the only way to get one is `verify_credential` (or its two
//! variants), which checked, in order, the clock, the shape, the certificate's window and chain, and the
//! credential key's signature. It has no public constructor, no `Clone`, no `Default`, no `From<&str>`.

use core::fmt;

use azlin_cbor::{decode, IntMap, IntMapBuilder};
use azlin_regbase::{
    AccountId, AnchorSet, CredentialNonce, DeviceId, FamilyId, KeyEpoch, LeaderTime, MemberId,
    MetaGroupId, PolicyEpoch, Signature64,
};
use azlin_seams::{Ed25519Verifier, SigAlg, SignatureVerifier};

use crate::canon::{build, bytes_of, preimage_of, write, Canonical, OUTER_LIMITS};
use crate::cert::Certificate;
use crate::claims::{CredentialClaims, SsoClaim};
use crate::error::{CanonicalError, CredError, WireError};
use crate::scopes::Scopes;
use crate::verify::{check_times, VerifierNow, VerifyRules};

/// The wire keys of a credential.
const K_CLAIMS: u64 = 1;
const K_CERT: u64 = 2;
const K_SIG: u64 = 3;

/// A credential as it travels: claims, the certificate of the credential key, the signature.
///
/// The claims it shows are UNVERIFIED: a `Credential` is what was received or issued, and only
/// [`verify_credential`] turns it into a [`VerifiedCredential`].
///
/// Implements planning/architecture.md section 5.11.
///
/// Caller: `CredentialIssuer::issue` (S9i's `finish_refresh` puts its bytes in the refresh answer),
/// `Credential::decode` (the nodes' connection handler, S6), S9h (opaque).
#[derive(Clone, PartialEq, Eq)]
pub struct Credential {
    claims: CredentialClaims,
    cert: Certificate,
    sig: Signature64,
    payload: Vec<u8>,
}

impl Credential {
    /// The credential of these claims, this certificate and this signature. The payload is the
    /// canonical bytes of `claims`; whether `sig` covers them is the verifier's question (the tests make
    /// credentials with a wrong signature through this).
    ///
    /// Caller: the tests; a holder that signs elsewhere (S9f) and hands back the signature.
    pub fn from_parts(
        claims: CredentialClaims,
        cert: Certificate,
        sig: Signature64,
    ) -> Result<Credential, CanonicalError> {
        let payload = claims.canonical_bytes()?;
        Ok(Credential {
            claims,
            cert,
            sig,
            payload,
        })
    }

    /// A credential whose payload is already the signed bytes of `claims` (the issuer's).
    pub(crate) fn assemble(
        claims: CredentialClaims,
        cert: Certificate,
        sig: Signature64,
        payload: Vec<u8>,
    ) -> Credential {
        Credential {
            claims,
            cert,
            sig,
            payload,
        }
    }

    /// The claims, UNVERIFIED.
    pub fn claims(&self) -> &CredentialClaims {
        &self.claims
    }

    /// The certificate of the credential key.
    pub fn certificate(&self) -> &Certificate {
        &self.cert
    }

    /// The credential key's signature.
    pub fn signature(&self) -> &Signature64 {
        &self.sig
    }

    /// The bytes the signature covers (the canonical claims).
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// The canonical bytes of the whole credential: the payload as it was signed, the certificate, the
    /// signature. A credential read with `decode` encodes back to the bytes it was read from.
    ///
    /// Caller: S9i (`finish_refresh` puts them in `RefreshAnswerWire::credential`), the tests.
    pub fn encode(&self) -> Result<Vec<u8>, CanonicalError> {
        let value = build(
            IntMapBuilder::new()
                .put(K_CLAIMS, bytes_of(&self.payload))
                .put(K_CERT, self.cert.to_value()?)
                .put(K_SIG, bytes_of(self.sig.as_bytes())),
        )?;
        write(&value, &OUTER_LIMITS)
    }

    /// The credential of canonical bytes, STRICTLY: the codec's checks, a certificate of exactly its
    /// keys, a signature of 64 bytes, claims that parse (an unknown scope kind is `UnknownScope`). A
    /// key of the wrapper or of the claims that nobody knows is ignored unless `crit` lists it.
    ///
    /// Caller: the nodes' connection handler (S6), S9g's tests, `verify_credential_bytes`.
    pub fn decode(bytes: &[u8]) -> Result<Credential, WireError> {
        let value = decode(bytes, &OUTER_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        let payload = map.req_bytes(K_CLAIMS)?;
        let cert = Certificate::from_value(map.req_value(K_CERT)?)?;
        let sig = Signature64::from_bytes(map.req_bytes_n::<64>(K_SIG)?);
        map.finish()?;
        let claims = CredentialClaims::decode_payload(payload)?;
        Ok(Credential {
            claims,
            cert,
            sig,
            payload: payload.to_vec(),
        })
    }
}

/// Prints the claims (ids as four bytes), the certificate (no signature) and the payload's length: never
/// a signature byte.
impl fmt::Debug for Credential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Credential")
            .field("claims", &self.claims)
            .field("cert", &self.cert)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

/// A credential that `verify_credential` accepted: its claims, read from the signed bytes, and the
/// metadata group whose identity key certified the credential key.
///
/// A witness. It has no public constructor: the only way to hold one is a successful verification.
/// It has no `Clone`, `Default`, `Deserialize` or `From<&str>` either; pass it by reference.
///
/// ```compile_fail,E0599
/// // no `Clone`: a verified value is not copied around
/// fn copy(v: azlin_creds::VerifiedCredential) {
///     let _ = v.clone();
/// }
/// ```
///
/// ```compile_fail,E0599
/// // no `Default`: there is no verified credential of nothing
/// let _ = azlin_creds::VerifiedCredential::default();
/// ```
///
/// ```compile_fail,E0624
/// // no public constructor: the constructor is private to the crate
/// let _ = azlin_creds::VerifiedCredential::new(todo!(), todo!());
/// ```
///
/// ```compile_fail,E0277
/// // no `From<&str>`: a verified credential is not made from text
/// let _: azlin_creds::VerifiedCredential = "a credential".into();
/// ```
///
/// The control compiles: a verified credential is read by reference.
///
/// ```
/// fn read(v: &azlin_creds::VerifiedCredential) -> azlin_regbase::AccountId {
///     v.account()
/// }
/// ```
///
/// Implements planning/architecture.md section 2.4 ("each node also checks each connection"), 5.11.
///
/// Caller: S9g (`check_connection` takes `&VerifiedCredential`), the nodes' verb handlers (S6).
pub struct VerifiedCredential {
    claims: CredentialClaims,
    group: MetaGroupId,
}

impl VerifiedCredential {
    /// Made only by a verification.
    pub(crate) fn new(claims: CredentialClaims, group: MetaGroupId) -> VerifiedCredential {
        VerifiedCredential { claims, group }
    }

    /// The account.
    pub fn account(&self) -> AccountId {
        self.claims.account
    }

    /// The member.
    pub fn member(&self) -> MemberId {
        self.claims.member
    }

    /// The device; `None` for an app principal, a link or a request.
    pub fn device(&self) -> Option<DeviceId> {
        self.claims.device
    }

    /// The family whose token the refresh spent; `None` for an app principal and a link.
    pub fn family(&self) -> Option<FamilyId> {
        self.claims.family
    }

    /// The policy epoch the credential was issued under (S9g refuses an older one).
    pub fn policy_epoch(&self) -> PolicyEpoch {
        self.claims.policy_epoch
    }

    /// The scopes, untouched: `None` for a device credential (the registry and the grants decide).
    pub fn scopes(&self) -> Option<&Scopes> {
        self.claims.scopes.as_ref()
    }

    /// When the credential was issued.
    pub fn issued_at(&self) -> LeaderTime {
        self.claims.issued_at
    }

    /// The first second it is no longer valid.
    pub fn expires_at(&self) -> LeaderTime {
        self.claims.expires_at
    }

    /// The epoch of the credential key that signed it (S9g refuses a revoked epoch).
    pub fn key_epoch(&self) -> KeyEpoch {
        self.claims.key_epoch
    }

    /// The nonce a revocation of this credential names (S9g refuses a revoked nonce).
    pub fn nonce(&self) -> CredentialNonce {
        self.claims.nonce
    }

    /// The single sign-on fact.
    pub fn sso(&self) -> Option<&SsoClaim> {
        self.claims.sso.as_ref()
    }

    /// The metadata group whose identity key certified the credential key. THE CALLER MUST check that
    /// the credential's account lives in this group (the account-to-group placement is the map's): a
    /// voter of another group holds a valid credential key too and could otherwise vouch for an account
    /// that is not its own (drift row X-DELTA-W1S9E-3).
    pub fn group(&self) -> MetaGroupId {
        self.group
    }

    /// All the claims.
    pub fn claims(&self) -> &CredentialClaims {
        &self.claims
    }
}

impl fmt::Debug for VerifiedCredential {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedCredential")
            .field("claims", &self.claims)
            .field("group", &self.group)
            .finish()
    }
}

/// Does NOT consult revocations: this verifies a signature chain and a lifetime, nothing about the
/// registry. A credential whose nonce, family, device, member or key epoch is revoked still verifies
/// here; S9g's `check_connection` refuses it against the registry.
///
/// What it checks, in this order, and the first failure is the answer: the node's clock is trusted
/// (`ClockUntrusted`); the signed bytes parse and the claims are well shaped, `expires_at` after
/// `issued_at`, at most `max_credential_lifetime`, a credential of no device carries scopes and a device
/// credential names its family (`Malformed`, `LifetimeTooLong`, `UnknownScope`); the certificate's window
/// is sane (`CertMalformed`); the credential's `key_epoch` is the certificate's (`CertMismatch`); not
/// before `issued_at - skew` (`NotYetValid`) and before `expires_at` (`Expired`); the certificate's
/// window covers `issued_at` and the node's now (`CertWindow`); the certificate's signer is an identity
/// key of its group in the anchors (`UnknownIdentity`) whose window covers the certificate's
/// `not_before` and the node's now (`IdentityWindow`); the identity key's signature over the certificate
/// (`BadCert`) and the credential key's signature over the claims (`BadSignature`) verify. Two signature
/// verifications in all.
///
/// An empty anchor set refuses everything (`UnknownIdentity`): never "no check".
///
/// Implements planning/architecture.md section 2.4 ("Credentials are verifiable on every node") and
/// section 5.11.
///
/// Caller: the nodes' connection handler (S6) through S9g's `check_connection`; S9f's probe through
/// `CredentialIssuer::new`; the tests.
pub fn verify_credential(
    c: &Credential,
    anchors: &AnchorSet,
    now: VerifierNow,
    rules: &VerifyRules,
) -> Result<VerifiedCredential, CredError> {
    verify_credential_with(&Ed25519Verifier, c, anchors, now, rules)
}

/// `verify_credential` over a signature verifier of the caller's: the seam of the cost test (a counting
/// verifier shows that one credential costs two verifications) and of a different strict scheme. Never
/// pass `FakeScheme` in production: anybody can forge it.
///
/// Seam: the test S9E-22; production uses [`verify_credential`].
pub fn verify_credential_with(
    verifier: &dyn SignatureVerifier,
    c: &Credential,
    anchors: &AnchorSet,
    now: VerifierNow,
    rules: &VerifyRules,
) -> Result<VerifiedCredential, CredError> {
    let now = now.decide()?;
    // The verified claims are the signed bytes' own, never the copy the value carries.
    let claims = CredentialClaims::decode_payload(&c.payload)?;
    claims.check_shape(rules)?;
    let cert = &c.cert;
    if !cert.shape_ok() {
        return Err(CredError::CertMalformed);
    }
    if claims.key_epoch != cert.epoch {
        return Err(CredError::CertMismatch);
    }
    check_times(claims.issued_at, claims.expires_at, now, rules.skew)?;
    if !cert.window_covers(claims.issued_at, now) {
        return Err(CredError::CertWindow);
    }
    cert.check_identity(anchors, Some(now))?;
    cert.check_signature(verifier)?;
    let preimage = preimage_of::<CredentialClaims>(&c.payload);
    let verdict = verifier.verify(
        SigAlg::Ed25519,
        cert.cred_pub.as_bytes(),
        &preimage,
        c.sig.as_bytes(),
    );
    if verdict.is_err() {
        return Err(CredError::BadSignature);
    }
    Ok(VerifiedCredential::new(claims, cert.group))
}

/// Reads a credential from its bytes and verifies it: `ClockUntrusted` first (an untrusted clock refuses
/// even bytes that do not parse), then the refusals of `Credential::decode` as `Malformed` (an unknown
/// scope kind as `UnknownScope`), then `verify_credential`.
///
/// Caller: the nodes' connection handler (S6): the bytes of a request's credential in one call.
pub fn verify_credential_bytes(
    bytes: &[u8],
    anchors: &AnchorSet,
    now: VerifierNow,
    rules: &VerifyRules,
) -> Result<VerifiedCredential, CredError> {
    now.decide()?;
    let credential = Credential::decode(bytes)?;
    verify_credential(&credential, anchors, now, rules)
}
