//! Member assertion: "signed by the metadata group; account, member, device, role, groups, issued,
//! expiry (minutes). **This is how services learn membership without storage access**: the device
//! fetches one and presents it (meet org mode, the mail sending relay, the identity service's admin API).
//! A removed member gets no new one, so offboarding takes effect within the assertion lifetime. Services
//! verify it against the cluster's map anchors (static config) and the account id set at install"
//! (planning/architecture.md section 5.11 "Member assertion (`azlin-member-assert-v1`, `member_assert`
//! verb)").
//!
//! An [`Assertion`] has the shape of a credential (`credential.rs`): the signed payload kept verbatim,
//! the certificate of the credential key that signed it, the signature. It is signed by the SAME
//! credential key under its OWN domain (`azlin-member-assert-v1`), so a signature of one is never a
//! signature of the other. It lives at most ten minutes (DRIFT D-12), and a verifier REFUSES a list of
//! groups that is not strictly ascending or has more than `MAX_GROUPS` entries: two encodings of one set
//! never both verify.
//!
//! [`VerifiedAssertion`] is a witness: only `verify_assertion` makes one.

use core::fmt;

use azlin_cbor::{decode, IntMap, IntMapBuilder};
use azlin_regbase::{
    AccountId, AccountRole, AnchorSet, DeviceId, LeaderTime, MemberId, MetaGroupId, Signature64,
    UserGroupId,
};
use azlin_seams::{Ed25519Verifier, SigAlg, SignatureVerifier};

use crate::canon::{build, bytes_of, preimage_of, write, Canonical, OUTER_LIMITS};
use crate::cert::Certificate;
use crate::claims::AssertionClaims;
use crate::error::{AssertError, CanonicalError, WireError};
use crate::groups::{check_placement, AccountGroups};
use crate::verify::{check_lifetime, check_times, VerifierNow, VerifyRules};

/// The wire keys of an assertion.
const K_CLAIMS: u64 = 1;
const K_CERT: u64 = 2;
const K_SIG: u64 = 3;

/// An assertion as it travels: claims, the certificate of the credential key, the signature.
///
/// The claims it shows are UNVERIFIED: only [`verify_assertion`] turns it into a [`VerifiedAssertion`].
///
/// Implements planning/architecture.md section 5.11.
///
/// Caller: `AssertionIssuer::issue` (S9g's `assertion_claims` builds the claims, the `member_assert` verb
/// returns the bytes), `Assertion::decode` and `verify_assertion` (the services A2 to A5).
#[derive(Clone, PartialEq, Eq)]
pub struct Assertion {
    claims: AssertionClaims,
    cert: Certificate,
    sig: Signature64,
    payload: Vec<u8>,
}

impl Assertion {
    /// The assertion of these claims, this certificate and this signature. The payload is the canonical
    /// bytes of `claims` (refused when the groups are not in their one canonical form); whether `sig`
    /// covers them is the verifier's question.
    ///
    /// Caller: the tests; a holder that signs elsewhere and hands back the signature.
    pub fn from_parts(
        claims: AssertionClaims,
        cert: Certificate,
        sig: Signature64,
    ) -> Result<Assertion, CanonicalError> {
        let payload = claims.canonical_bytes()?;
        Ok(Assertion {
            claims,
            cert,
            sig,
            payload,
        })
    }

    /// An assertion whose payload is already the signed bytes of `claims` (the issuer's).
    pub(crate) fn assemble(
        claims: AssertionClaims,
        cert: Certificate,
        sig: Signature64,
        payload: Vec<u8>,
    ) -> Assertion {
        Assertion {
            claims,
            cert,
            sig,
            payload,
        }
    }

    /// The claims, UNVERIFIED.
    pub fn claims(&self) -> &AssertionClaims {
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

    /// The canonical bytes of the whole assertion; an assertion read with `decode` encodes back to the
    /// bytes it was read from.
    ///
    /// Caller: the `member_assert` verb's answer (S6), the tests.
    pub fn encode(&self) -> Result<Vec<u8>, CanonicalError> {
        let value = build(
            IntMapBuilder::new()
                .put(K_CLAIMS, bytes_of(&self.payload))
                .put(K_CERT, self.cert.to_value()?)
                .put(K_SIG, bytes_of(self.sig.as_bytes())),
        )?;
        write(&value, &OUTER_LIMITS)
    }

    /// The assertion of canonical bytes, STRICTLY (the codec's checks, a certificate of exactly its
    /// keys, claims that parse with their groups in the canonical form).
    ///
    /// Caller: the services A2 to A5 (the bytes a device presents), `verify_assertion_bytes`.
    pub fn decode(bytes: &[u8]) -> Result<Assertion, WireError> {
        let value = decode(bytes, &OUTER_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        let payload = map.req_bytes(K_CLAIMS)?;
        let cert = Certificate::from_value(map.req_value(K_CERT)?)?;
        let sig = Signature64::from_bytes(map.req_bytes_n::<64>(K_SIG)?);
        map.finish()?;
        let claims = AssertionClaims::decode_payload(payload)?;
        Ok(Assertion {
            claims,
            cert,
            sig,
            payload: payload.to_vec(),
        })
    }
}

/// Prints the claims (ids as four bytes, the groups as a count), the certificate (no signature) and the
/// payload's length: never a signature byte.
impl fmt::Debug for Assertion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Assertion")
            .field("claims", &self.claims)
            .field("cert", &self.cert)
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

/// An assertion that `verify_assertion` accepted: its claims, read from the signed bytes, and the
/// metadata group whose identity key certified the credential key.
///
/// A witness. It has no public constructor, no `Clone`, no `Default`, no `Deserialize` and no
/// `From<&str>`:
///
/// ```compile_fail,E0599
/// // no `Clone`
/// fn copy(v: azlin_creds::VerifiedAssertion) {
///     let _ = v.clone();
/// }
/// ```
///
/// ```compile_fail,E0599
/// // no `Default`
/// let _ = azlin_creds::VerifiedAssertion::default();
/// ```
///
/// ```compile_fail,E0624
/// // no public constructor
/// let _ = azlin_creds::VerifiedAssertion::new(todo!(), todo!());
/// ```
///
/// ```compile_fail,E0277
/// // no `From<&str>`
/// let _: azlin_creds::VerifiedAssertion = "an assertion".into();
/// ```
///
/// The control compiles:
///
/// ```
/// fn read(v: &azlin_creds::VerifiedAssertion) -> azlin_regbase::AccountId {
///     v.account()
/// }
/// ```
///
/// Implements planning/architecture.md section 5.11.
///
/// Caller: the services A2 to A5 (meet in org mode, the mail relays, the identity service's admin API).
pub struct VerifiedAssertion {
    claims: AssertionClaims,
    group: MetaGroupId,
}

impl VerifiedAssertion {
    /// Made only by a verification.
    pub(crate) fn new(claims: AssertionClaims, group: MetaGroupId) -> VerifiedAssertion {
        VerifiedAssertion { claims, group }
    }

    /// The account (the one this service was installed for).
    pub fn account(&self) -> AccountId {
        self.claims.account
    }

    /// The member.
    pub fn member(&self) -> MemberId {
        self.claims.member
    }

    /// The device the assertion was fetched for.
    pub fn device(&self) -> DeviceId {
        self.claims.device
    }

    /// The member's role.
    pub fn role(&self) -> AccountRole {
        self.claims.role
    }

    /// The user groups of the member: sorted, without duplicates.
    pub fn groups(&self) -> &[UserGroupId] {
        &self.claims.groups
    }

    /// When the assertion was issued.
    pub fn issued(&self) -> LeaderTime {
        self.claims.issued
    }

    /// The first second it is no longer valid.
    pub fn expires(&self) -> LeaderTime {
        self.claims.expires
    }

    /// The metadata group whose identity key certified the credential key, and, because
    /// `verify_assertion` checked it against the verifier's `AccountGroups`, the group the account
    /// lives on (drift row X-DELTA-W1S9E-3: a voter of another group is `GroupMismatch`).
    pub fn group(&self) -> MetaGroupId {
        self.group
    }

    /// All the claims.
    pub fn claims(&self) -> &AssertionClaims {
        &self.claims
    }
}

impl fmt::Debug for VerifiedAssertion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedAssertion")
            .field("claims", &self.claims)
            .field("group", &self.group)
            .finish()
    }
}

/// Does NOT consult the registry: a member that was removed a minute ago still verifies until the
/// assertion expires (at most ten minutes: "offboarding takes effect within the assertion lifetime").
///
/// What it checks, in this order, and the first failure is the answer: the node's clock is trusted
/// (`ClockUntrusted`); the signed bytes parse, the groups are strictly ascending and at most
/// `MAX_GROUPS`, `expires` is after `issued` (`Malformed`) and at most `max_assertion_lifetime` after it
/// (`LifetimeTooLong`); the account is `installed_account` (`WrongAccount`); the certificate's window is
/// sane (`CertMalformed`); not before `issued - skew` (`NotYetValid`) and before `expires` (`Expired`);
/// the certificate's window covers `issued` and the node's now (`CertWindow`); the certificate's signer is
/// an identity key of its group in the anchors (`UnknownIdentity`) whose window covers the certificate's
/// `not_before` and the node's now (`IdentityWindow`); the identity key's signature over the
/// certificate (`BadCert`) and the credential key's signature over the claims (`BadSignature`) verify;
/// and last, once the chain is authentic, the placement: `groups` knows the account (`UnknownAccount`)
/// and names the group whose identity key certified the credential key (`GroupMismatch`, an ALARM: a
/// voter of another group is vouching for an account that is not its own).
///
/// `groups` is REQUIRED (drift row X-DELTA-W1S9E-3, decision D-W1S9E3), and a service builds it from its
/// install config: an `InstalledAccount` of the account id and the group the map places it on. The
/// placement runs after the signatures so that a forgery cannot raise the alarm.
///
/// An empty anchor set refuses everything (`UnknownIdentity`): never "no check".
///
/// Implements planning/architecture.md section 5.11 ("Services verify it against the cluster's map
/// anchors (static config) and the account id set at install") and section 2.4 ("Accounts map to a
/// metadata group").
///
/// Caller: the services A2 to A5 (meet in org mode, the mail sending relay, the identity service's admin
/// API), with `InstalledAccount` as `groups`.
pub fn verify_assertion(
    a: &Assertion,
    anchors: &AnchorSet,
    groups: &dyn AccountGroups,
    now: VerifierNow,
    rules: &VerifyRules,
    installed_account: &AccountId,
) -> Result<VerifiedAssertion, AssertError> {
    verify_assertion_with(
        &Ed25519Verifier,
        a,
        anchors,
        groups,
        now,
        rules,
        installed_account,
    )
}

/// `verify_assertion` over a signature verifier of the caller's (see `verify_credential_with`).
///
/// Seam: tests; production uses [`verify_assertion`].
pub fn verify_assertion_with(
    verifier: &dyn SignatureVerifier,
    a: &Assertion,
    anchors: &AnchorSet,
    groups: &dyn AccountGroups,
    now: VerifierNow,
    rules: &VerifyRules,
    installed_account: &AccountId,
) -> Result<VerifiedAssertion, AssertError> {
    let (claims, certified) =
        verify_chain_with(verifier, a, anchors, now, rules, installed_account)?;
    check_placement(groups, &claims.account, certified)?;
    Ok(VerifiedAssertion::new(claims, certified))
}

/// Everything `verify_assertion` checks but the placement of the account (see
/// `credential::verify_chain_with`): the clock, the shape, the installed account, the chain and both
/// signatures; the claims are read from the signed bytes, with the group whose identity key certified
/// the credential key. Crate-internal: the issuers' probe tests a key, not an account.
pub(crate) fn verify_chain_with(
    verifier: &dyn SignatureVerifier,
    a: &Assertion,
    anchors: &AnchorSet,
    now: VerifierNow,
    rules: &VerifyRules,
    installed_account: &AccountId,
) -> Result<(AssertionClaims, MetaGroupId), AssertError> {
    let now = now.decide()?;
    // The verified claims are the signed bytes' own, never the copy the value carries.
    let claims = AssertionClaims::decode_payload(&a.payload)?;
    check_lifetime(claims.issued, claims.expires, rules.max_assertion_lifetime)?;
    if claims.account != *installed_account
        && !azlin_seams::mutant!("creds_assertion_account_unchecked")
    {
        return Err(AssertError::WrongAccount);
    }
    let cert = &a.cert;
    if !cert.shape_ok() {
        return Err(AssertError::CertMalformed);
    }
    check_times(claims.issued, claims.expires, now, rules.skew)?;
    if !cert.window_covers(claims.issued, now) {
        return Err(AssertError::CertWindow);
    }
    cert.check_identity(anchors, Some(now))?;
    cert.check_signature(verifier)?;
    let preimage = preimage_of::<AssertionClaims>(&a.payload);
    let verdict = verifier.verify(
        SigAlg::Ed25519,
        cert.cred_pub.as_bytes(),
        &preimage,
        a.sig.as_bytes(),
    );
    if verdict.is_err() {
        return Err(AssertError::BadSignature);
    }
    Ok((claims, cert.group))
}

/// Reads an assertion from its bytes and verifies it: `ClockUntrusted` first, then the refusals of
/// `Assertion::decode` as `Malformed`, then `verify_assertion` (with its placement check).
///
/// Caller: the services A2 to A5: the bytes a device presents, in one call.
pub fn verify_assertion_bytes(
    bytes: &[u8],
    anchors: &AnchorSet,
    groups: &dyn AccountGroups,
    now: VerifierNow,
    rules: &VerifyRules,
    installed_account: &AccountId,
) -> Result<VerifiedAssertion, AssertError> {
    now.decide()?;
    let assertion = Assertion::decode(bytes)?;
    verify_assertion(&assertion, anchors, groups, now, rules, installed_account)
}
