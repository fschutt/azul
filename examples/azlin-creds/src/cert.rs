//! The certificate of a credential key (planning/architecture.md section 2.4: "credentials are signed by
//! the metadata group's credential key. The map names the group's long-lived **identity key**, which
//! certifies each credential key, and every credential carries that certificate (delegated)"; "The
//! credential key rotates every 30 days without a map signature: the identity key certifies the new
//! one"; DRIFT D-11).
//!
//! A [`Certificate`] says: "the identity key `identity_pub` of metadata group `group` certifies that
//! `cred_pub` is the credential key of epoch `epoch` from `not_before` until `not_after`". The identity
//! key signs the BODY (everything but the signature) under the domain `azlin-credential-cert-v1`.
//!
//! The certificate NAMES its signer (`identity_pub`, inside the signed body). That is how a verifier
//! tells a certificate signed by a key that is no identity key of the group (`UnknownIdentity`) from one
//! that was tampered with (`BadSignature`): without the name the two are the same cryptographic fact
//! (drift row X-DELTA-W1S9E-2). The name grants nothing: the key must still be in the anchors, for the
//! certificate's group, with a window that covers the certificate.
//!
//! The window is `[not_before, not_after)`; D-11 gives it as `[active_from - lead, active_until +
//! max_credential_lifetime + skew]`, so that the previous key lives exactly as long as the last
//! credential it signed (S9f schedules it). A window is sane when `not_after > not_before` and the span
//! is at most [`MAX_CERT_SPAN`] (45 days: the rotation period, the lead and the overlap).
//!
//! The wire form is a CLOSED schema: a flat map of keys 1 to 7 (group, epoch, `cred_pub`,
//! `identity_pub`, `not_before`, `not_after`, signature); a key it does not know, and `crit`, are
//! refused. A certificate is the one object every verifier must read the same way for a whole rotation,
//! and a field a newer issuer adds to it would be a constraint, which an older verifier must refuse.

use azlin_cbor::{decode, IntMap, IntMapBuilder, Value};
use azlin_keys::Span;
use azlin_regbase::{AnchorSet, KeyEpoch, LeaderTime, MetaGroupId, PublicKey32, Signature64};
use azlin_seams::{Ed25519Verifier, SigAlg, SignatureVerifier};
use core::fmt;

use crate::canon::{
    build, bytes_of, domain_of, finish_closed, preimage_of, read_time, read_u16, read_u32, uint_of,
    write, Canonical, Short, CREDENTIAL_CERT_V1, OUTER_LIMITS,
};
use crate::error::{CanonicalError, CertError, WireError};
use crate::signer::{sign_checked, SignFailure, Signer};
use crate::verify::{window_holds, NodeTime};

/// The longest window of a certificate: 45 days (the credential key rotates every 30 days, is certified
/// a week ahead, and the previous key lives one overlap longer: DRIFT D-11).
pub const MAX_CERT_SPAN: Span = Span::days(45);

/// The wire keys of a certificate.
const K_GROUP: u64 = 1;
const K_EPOCH: u64 = 2;
const K_CRED_PUB: u64 = 3;
const K_IDENTITY_PUB: u64 = 4;
const K_NOT_BEFORE: u64 = 5;
const K_NOT_AFTER: u64 = 6;
const K_SIG: u64 = 7;

/// An identity key's certificate of a credential key.
///
/// Implements planning/architecture.md section 2.4 and DRIFT D-11.
///
/// Caller: `certify` (S9f's rotation makes them), `verify_cert` (S9f's `StageCredential`), every
/// `Credential` and `Assertion` carries one; S9f reads `group`, `epoch` and `cred_pub`.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Certificate {
    /// The metadata group whose identity key certified.
    pub group: MetaGroupId,
    /// The epoch of the credential key (a credential's `key_epoch` must equal it).
    pub epoch: KeyEpoch,
    /// The credential key being certified.
    pub cred_pub: PublicKey32,
    /// The identity key that signed (named inside the signed body).
    pub identity_pub: PublicKey32,
    /// The first second the certificate is valid (inclusive).
    pub not_before: LeaderTime,
    /// The first second it is no longer valid (exclusive).
    pub not_after: LeaderTime,
    /// The identity key's signature over the body, under `azlin-credential-cert-v1`.
    pub identity_sig: Signature64,
}

/// Prints the group, the epoch, four bytes of each key and the window: never the signature.
impl fmt::Debug for Certificate {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Certificate")
            .field("group", &self.group)
            .field("epoch", &self.epoch)
            .field("cred_pub", &Short(self.cred_pub.as_bytes()))
            .field("identity_pub", &Short(self.identity_pub.as_bytes()))
            .field("not_before", &self.not_before)
            .field("not_after", &self.not_after)
            .finish()
    }
}

/// Whether `[not_before, not_after)` is a sane window: at least a second, at most [`MAX_CERT_SPAN`].
fn window_is_sane(not_before: LeaderTime, not_after: LeaderTime) -> bool {
    match not_after.elapsed_since(not_before) {
        Some(span) => span > Span::ZERO && span <= MAX_CERT_SPAN,
        None => false,
    }
}

impl Certificate {
    /// The map of the body: keys 1 to 6.
    fn body_builder(&self) -> IntMapBuilder {
        IntMapBuilder::new()
            .put(K_GROUP, uint_of(u64::from(self.group.get())))
            .put(K_EPOCH, uint_of(u64::from(self.epoch.get())))
            .put(K_CRED_PUB, bytes_of(self.cred_pub.as_bytes()))
            .put(K_IDENTITY_PUB, bytes_of(self.identity_pub.as_bytes()))
            .put(K_NOT_BEFORE, uint_of(self.not_before.as_secs()))
            .put(K_NOT_AFTER, uint_of(self.not_after.as_secs()))
    }

    /// The certificate as a CBOR map (body and signature), for embedding in a credential.
    ///
    /// Caller: `Credential::encode`, `Assertion::encode`, `Certificate::encode`.
    pub fn to_value(&self) -> Result<Value, CanonicalError> {
        build(
            self.body_builder()
                .put(K_SIG, bytes_of(self.identity_sig.as_bytes())),
        )
    }

    /// The certificate of a CBOR map, STRICTLY: every key present, none unknown, no `crit`.
    ///
    /// Caller: `Credential::decode`, `Assertion::decode`, `Certificate::decode`.
    pub fn from_value(value: &Value) -> Result<Certificate, WireError> {
        let mut map = IntMap::new(value)?;
        let group = MetaGroupId::new(read_u16(&mut map, K_GROUP)?);
        let epoch = KeyEpoch::new(read_u32(&mut map, K_EPOCH)?);
        let cred_pub = PublicKey32::from_bytes(map.req_bytes_n::<32>(K_CRED_PUB)?);
        let identity_pub = PublicKey32::from_bytes(map.req_bytes_n::<32>(K_IDENTITY_PUB)?);
        let not_before = read_time(&mut map, K_NOT_BEFORE)?;
        let not_after = read_time(&mut map, K_NOT_AFTER)?;
        let identity_sig = Signature64::from_bytes(map.req_bytes_n::<64>(K_SIG)?);
        finish_closed(map)?;
        Ok(Certificate {
            group,
            epoch,
            cred_pub,
            identity_pub,
            not_before,
            not_after,
            identity_sig,
        })
    }

    /// The certificate as its canonical bytes: what S9f stores in `reg_key_epoch.cert`.
    ///
    /// Caller: S9f (stores and replicates the certificate), the golden vectors.
    pub fn encode(&self) -> Result<Vec<u8>, CanonicalError> {
        write(&self.to_value()?, &OUTER_LIMITS)
    }

    /// The certificate of canonical bytes, STRICTLY (see [`Certificate::from_value`]).
    ///
    /// Caller: S9f (reads the stored certificate back), the tests.
    pub fn decode(bytes: &[u8]) -> Result<Certificate, WireError> {
        let value = decode(bytes, &OUTER_LIMITS)?;
        Certificate::from_value(&value)
    }

    /// Whether the window is sane (see the module).
    pub(crate) fn shape_ok(&self) -> bool {
        window_is_sane(self.not_before, self.not_after)
    }

    /// Whether the window covers a credential issued at `issued` and checked at the node's `now`: both
    /// inside `[not_before, not_after)`.
    pub(crate) fn window_covers(&self, issued: LeaderTime, now: NodeTime) -> bool {
        self.not_before <= issued
            && issued < self.not_after
            && window_holds(self.not_before, Some(self.not_after), now)
    }

    /// The anchors' verdict on the signing key: it must be an identity key of THIS group
    /// (`UnknownIdentity` when it is not, and when the anchors hold none), and a window of that key must
    /// cover the moment the certificate was made (`not_before`) and, when `now` is given, the moment of
    /// the check (`IdentityWindow` when it does not). No signature is checked here.
    pub(crate) fn check_identity(
        &self,
        anchors: &AnchorSet,
        now: Option<NodeTime>,
    ) -> Result<(), CertError> {
        let mut key_known = false;
        let mut covers_not_before = false;
        let mut covers_now = now.is_none();
        for (group, window) in &anchors.identity {
            let same_group = *group == self.group;
            if !same_group && !azlin_seams::mutant!("creds_identity_any_group") {
                continue;
            }
            if window.public != self.identity_pub {
                continue;
            }
            key_known = true;
            if window.contains(self.not_before) {
                covers_not_before = true;
            }
            if let Some(now) = now {
                if window_holds(window.not_before, window.not_after, now) {
                    covers_now = true;
                }
            }
        }
        if !key_known {
            return Err(CertError::UnknownIdentity);
        }
        if !(covers_not_before && covers_now) {
            return Err(CertError::IdentityWindow);
        }
        Ok(())
    }

    /// The identity key's signature over the body.
    pub(crate) fn check_signature(
        &self,
        verifier: &dyn SignatureVerifier,
    ) -> Result<(), CertError> {
        let body = self
            .canonical_bytes()
            .map_err(|_| CertError::BadSignature)?;
        let preimage = preimage_of::<Certificate>(&body);
        verifier
            .verify(
                SigAlg::Ed25519,
                self.identity_pub.as_bytes(),
                &preimage,
                self.identity_sig.as_bytes(),
            )
            .map_err(|_| CertError::BadSignature)
    }
}

impl Canonical for Certificate {
    const DOMAIN: azlin_regbase::Domain = CREDENTIAL_CERT_V1;

    /// The body: keys 1 to 6, without the signature.
    fn canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        write(&build(self.body_builder())?, &OUTER_LIMITS)
    }
}

impl From<SignFailure> for CertError {
    fn from(f: SignFailure) -> CertError {
        match f {
            SignFailure::Sign(e) => CertError::Sign(e),
            SignFailure::Fault => CertError::SignerFault,
        }
    }
}

/// The identity key's certificate of a credential key: the window must be sane (`CertError::Malformed`
/// otherwise); the body is signed through `identity` under `azlin-credential-cert-v1`, and the
/// signature is checked against the signer's own public key before the certificate is returned.
///
/// Implements planning/architecture.md section 2.4 ("the identity key certifies the new one") and
/// DRIFT D-11.
///
/// Caller: S9f (the leader certifies the next credential key with the identity key it holds in
/// memory), S9-int; the tests.
pub fn certify(
    identity: &dyn Signer,
    group: MetaGroupId,
    epoch: KeyEpoch,
    cred_pub: PublicKey32,
    not_before: LeaderTime,
    not_after: LeaderTime,
) -> Result<Certificate, CertError> {
    if !window_is_sane(not_before, not_after) {
        return Err(CertError::Malformed);
    }
    let mut cert = Certificate {
        group,
        epoch,
        cred_pub,
        identity_pub: identity.public(),
        not_before,
        not_after,
        identity_sig: Signature64::from_bytes([0u8; 64]),
    };
    let body = cert.canonical_bytes().map_err(CertError::Encode)?;
    cert.identity_sig = sign_checked(identity, &domain_of::<Certificate>(), &body)?;
    Ok(cert)
}

/// Whether the certificate is one the anchors vouch for: the window is sane (`Malformed`), its signer
/// is an identity key of the certificate's group (`UnknownIdentity`) whose window covers the moment the
/// certificate was made, `not_before` (`IdentityWindow`), and the signature verifies (`BadSignature`).
/// An empty anchor set refuses everything. It does not look at the clock: whether the certificate is
/// valid NOW is `verify_credential`'s question.
///
/// Implements planning/architecture.md section 2.4 ("the identity key certifies the new one").
///
/// Caller: S9f (`StageCredential` verifies the certificate of a staged key against the current
/// anchors), S9g.
pub fn verify_cert(c: &Certificate, anchors: &AnchorSet) -> Result<(), CertError> {
    if !c.shape_ok() {
        return Err(CertError::Malformed);
    }
    c.check_identity(anchors, None)?;
    c.check_signature(&Ed25519Verifier)
}
