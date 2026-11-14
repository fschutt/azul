//! The issuer side (planning/architecture.md section 2.4: "The voters hold the identity key and the
//! credential key"; section 1.4 rule 9: "**New credentials are tested before they are used.** A rotated
//! token, a moved database or a new key is tried first ... Until the test passes the old one stays, and
//! the failure shows in the status line").
//!
//! A [`CredentialIssuer`] and an [`AssertionIssuer`] are made only after a PROBE credential (or
//! assertion), signed by the signer they were given under the certificate they were given, verified under
//! the anchors at the given time: a certificate that does not verify, a window that does not cover the
//! moment, anchors that do not know the identity key, or a signer whose public key is not the
//! certificate's `cred_pub` all fail the probe, and NO issuer value exists. So a key that cannot issue a
//! credential every node accepts is never put to use: S9f keeps the old key and shows the failure.
//!
//! `issue` takes the claims WITHOUT the key epoch and stamps it from the certificate: a caller cannot
//! pass another. It refuses what every verifier would refuse (the shape rules, a lifetime above the
//! rules' maximum, a credential that would outlive its certificate), and checks the signature the
//! holder returned against the holder's own public key before the credential leaves.

use azlin_keys::Span;
use azlin_regbase::{
    AccountId, AccountRole, AnchorSet, CredentialNonce, DeviceId, KeyEpoch, LeaderTime, MemberId,
    PolicyEpoch, UserGroupId,
};

use crate::assertion::{verify_assertion, Assertion};
use crate::canon::{domain_of, Canonical};
use crate::cert::Certificate;
use crate::claims::{AssertionClaims, CredentialClaims, GrantClaims, MAX_GROUPS};
use crate::credential::{verify_credential, Credential};
use crate::error::{AssertError, CredError, IssueError, IssuerError, ProbeFailure};
use crate::scopes::{LogRef, Scope, Scopes};
use crate::signer::{sign_checked, SignFailure, Signer};
use crate::verify::{check_lifetime, crossing, VerifierNow, VerifyRules};

/// The bytes of every id of the probe: a value no registry hands out (the probe never leaves the issuer).
const PROBE_ID: [u8; 16] = [0xA7; 16];

impl From<SignFailure> for IssueError {
    fn from(f: SignFailure) -> IssueError {
        match f {
            SignFailure::Sign(e) => IssueError::Sign(e),
            SignFailure::Fault => IssueError::SignerFault,
        }
    }
}

/// What the probe found, from what `issue` refused.
fn probe_failure(e: IssueError) -> ProbeFailure {
    match e {
        IssueError::Claims(c) => ProbeFailure::Credential(c),
        IssueError::AssertionClaims(a) => ProbeFailure::Assertion(a),
        IssueError::OutsideCertificate => ProbeFailure::Credential(CredError::CertWindow),
        IssueError::Encode(e) => ProbeFailure::Encode(e),
        IssueError::Sign(e) => ProbeFailure::Sign(e),
        IssueError::SignerFault => ProbeFailure::SignerFault,
    }
}

/// The issuer of credentials of one credential key: its signer and its certificate.
///
/// Implements planning/architecture.md section 2.4 and section 1.4 rule 9.
///
/// Caller: S9f's rotation (`CredentialIssuer::new` is the probe before a staged key is activated),
/// S9i (`finish_refresh` issues the credential of a refresh grant), the tests.
pub struct CredentialIssuer<'a> {
    signer: &'a dyn Signer,
    cert: Certificate,
    rules: VerifyRules,
}

impl<'a> CredentialIssuer<'a> {
    /// The issuer of `signer` under `cert`, with the default rules, after the probe at `now` (the
    /// leader's stamp) under `anchors` succeeded.
    ///
    /// Caller: S9f (the probe before activation), S9i.
    pub fn new(
        signer: &'a dyn Signer,
        cert: Certificate,
        anchors: &AnchorSet,
        now: LeaderTime,
    ) -> Result<CredentialIssuer<'a>, IssuerError> {
        CredentialIssuer::new_with_rules(signer, cert, anchors, now, VerifyRules::default())
    }

    /// As [`CredentialIssuer::new`], with the rules (the lifetime a credential may have, the skew) of
    /// the node's `node_config`.
    pub fn new_with_rules(
        signer: &'a dyn Signer,
        cert: Certificate,
        anchors: &AnchorSet,
        now: LeaderTime,
        rules: VerifyRules,
    ) -> Result<CredentialIssuer<'a>, IssuerError> {
        let issuer = CredentialIssuer {
            signer,
            cert,
            rules,
        };
        if !azlin_seams::mutant!("creds_issuer_skips_probe") {
            issuer.probe(anchors, now)?;
        }
        Ok(issuer)
    }

    /// The probe: a credential one second long, issued through [`CredentialIssuer::issue`] and
    /// verified like any node would verify it.
    fn probe(&self, anchors: &AnchorSet, now: LeaderTime) -> Result<(), IssuerError> {
        let failed = IssuerError::ProbeFailed;
        let expires = now
            .checked_add(Span::secs(1))
            .ok_or_else(|| failed(ProbeFailure::Credential(CredError::Malformed)))?;
        let scopes = Scopes::new(vec![Scope::LogAppend {
            log: LogRef::from_bytes(PROBE_ID),
        }])
        .map_err(|_| failed(ProbeFailure::Credential(CredError::Malformed)))?;
        let grant = GrantClaims {
            account: AccountId::from_bytes(PROBE_ID),
            member: MemberId::from_bytes(PROBE_ID),
            device: None,
            family: None,
            policy_epoch: PolicyEpoch::ZERO,
            scopes: Some(scopes),
            issued_at: now,
            expires_at: expires,
            nonce: CredentialNonce::from_bytes(PROBE_ID),
            sso: None,
        };
        let credential = self.issue(grant).map_err(|e| failed(probe_failure(e)))?;
        verify_credential(
            &credential,
            anchors,
            VerifierNow::trusted(crossing(now)),
            &self.rules,
        )
        .map_err(|e| failed(ProbeFailure::Credential(e)))?;
        Ok(())
    }

    /// The certificate this issuer issues under.
    pub fn certificate(&self) -> &Certificate {
        &self.cert
    }

    /// The epoch every credential of this issuer names: the certificate's.
    pub fn key_epoch(&self) -> KeyEpoch {
        self.cert.epoch
    }

    /// The credential of these claims, signed. The key epoch is the certificate's. Refused: claims that
    /// break a rule of the shape, a lifetime above the rules' maximum, a credential that would not live
    /// inside its certificate's window, a holder that did not sign or whose signature does not verify.
    ///
    /// Caller: S9i (`finish_refresh`).
    pub fn issue(&self, grant: GrantClaims) -> Result<Credential, IssueError> {
        let claims = grant.with_key_epoch(self.cert.epoch);
        claims
            .check_shape(&self.rules)
            .map_err(IssueError::Claims)?;
        if claims.issued_at < self.cert.not_before || claims.expires_at > self.cert.not_after {
            return Err(IssueError::OutsideCertificate);
        }
        let payload = claims.canonical_bytes().map_err(IssueError::Encode)?;
        let sig = sign_checked(self.signer, &domain_of::<CredentialClaims>(), &payload)?;
        Ok(Credential::assemble(claims, self.cert, sig, payload))
    }
}

/// What the probe of an ASSERTION issuer found, from what `issue` refused.
fn assertion_probe_failure(e: IssueError) -> ProbeFailure {
    match e {
        IssueError::Claims(c) => ProbeFailure::Credential(c),
        IssueError::AssertionClaims(a) => ProbeFailure::Assertion(a),
        IssueError::OutsideCertificate => ProbeFailure::Assertion(AssertError::CertWindow),
        IssueError::Encode(e) => ProbeFailure::Encode(e),
        IssueError::Sign(e) => ProbeFailure::Sign(e),
        IssueError::SignerFault => ProbeFailure::SignerFault,
    }
}

/// The groups sorted and without duplicates: the one canonical form of the set.
fn normalized_groups(mut groups: Vec<UserGroupId>) -> Vec<UserGroupId> {
    groups.sort();
    groups.dedup();
    groups
}

/// The issuer of member assertions of one credential key: its signer and its certificate. The same key
/// signs credentials and assertions, each under its own domain.
///
/// Implements planning/architecture.md section 5.11 ("Member assertion ... signed by the metadata
/// group") and section 1.4 rule 9.
///
/// Caller: S9g's `assertion_claims` builds the claims; S9i's `member_assert` verb (S6) issues them
/// through this; S9f's rotation probes the key with it as with `CredentialIssuer`.
pub struct AssertionIssuer<'a> {
    signer: &'a dyn Signer,
    cert: Certificate,
    rules: VerifyRules,
}

impl<'a> AssertionIssuer<'a> {
    /// The issuer of `signer` under `cert`, with the default rules, after the probe at `now` (the
    /// leader's stamp) under `anchors` succeeded.
    ///
    /// Caller: S9i (the `member_assert` verb), S9f's rotation.
    pub fn new(
        signer: &'a dyn Signer,
        cert: Certificate,
        anchors: &AnchorSet,
        now: LeaderTime,
    ) -> Result<AssertionIssuer<'a>, IssuerError> {
        AssertionIssuer::new_with_rules(signer, cert, anchors, now, VerifyRules::default())
    }

    /// As [`AssertionIssuer::new`], with the rules (the lifetime an assertion may have, the skew) of the
    /// node's `node_config`.
    pub fn new_with_rules(
        signer: &'a dyn Signer,
        cert: Certificate,
        anchors: &AnchorSet,
        now: LeaderTime,
        rules: VerifyRules,
    ) -> Result<AssertionIssuer<'a>, IssuerError> {
        let issuer = AssertionIssuer {
            signer,
            cert,
            rules,
        };
        if !azlin_seams::mutant!("creds_assertion_issuer_skips_probe") {
            issuer.probe(anchors, now)?;
        }
        Ok(issuer)
    }

    /// The probe: an assertion one second long, issued through [`AssertionIssuer::issue`] and verified
    /// like any service would verify it.
    fn probe(&self, anchors: &AnchorSet, now: LeaderTime) -> Result<(), IssuerError> {
        let failed = IssuerError::ProbeFailed;
        let expires = now
            .checked_add(Span::secs(1))
            .ok_or_else(|| failed(ProbeFailure::Assertion(AssertError::Malformed)))?;
        let claims = AssertionClaims {
            account: AccountId::from_bytes(PROBE_ID),
            member: MemberId::from_bytes(PROBE_ID),
            device: DeviceId::from_bytes(PROBE_ID),
            role: AccountRole::Guest,
            groups: Vec::new(),
            issued: now,
            expires,
        };
        let assertion = self
            .issue(claims)
            .map_err(|e| failed(assertion_probe_failure(e)))?;
        verify_assertion(
            &assertion,
            anchors,
            VerifierNow::trusted(crossing(now)),
            &self.rules,
            &AccountId::from_bytes(PROBE_ID),
        )
        .map_err(|e| failed(ProbeFailure::Assertion(e)))?;
        Ok(())
    }

    /// The certificate this issuer issues under.
    pub fn certificate(&self) -> &Certificate {
        &self.cert
    }

    /// The assertion of these claims, signed. The groups are sorted and deduplicated here (more than
    /// `MAX_GROUPS` distinct ones are refused, never truncated); a lifetime above the rules' maximum,
    /// `expires` not after `issued`, an assertion that would not live inside its certificate's window,
    /// a holder that did not sign or whose signature does not verify are refused.
    ///
    /// Caller: S9i (the `member_assert` verb).
    pub fn issue(&self, mut claims: AssertionClaims) -> Result<Assertion, IssueError> {
        claims.groups = normalized_groups(claims.groups);
        if claims.groups.len() > MAX_GROUPS {
            return Err(IssueError::AssertionClaims(AssertError::Malformed));
        }
        check_lifetime(
            claims.issued,
            claims.expires,
            self.rules.max_assertion_lifetime,
        )
        .map_err(|c| IssueError::AssertionClaims(AssertError::from(c)))?;
        if claims.issued < self.cert.not_before || claims.expires > self.cert.not_after {
            return Err(IssueError::OutsideCertificate);
        }
        let payload = claims.canonical_bytes().map_err(IssueError::Encode)?;
        let sig = sign_checked(self.signer, &domain_of::<AssertionClaims>(), &payload)?;
        Ok(Assertion::assemble(claims, self.cert, sig, payload))
    }
}
