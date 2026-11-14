//! The issuer side (planning/architecture.md section 2.4: "The voters hold the identity key and the
//! credential key"; section 1.4 rule 9: "**New credentials are tested before they are used.** A rotated
//! token, a moved database or a new key is tried first ... Until the test passes the old one stays, and
//! the failure shows in the status line").
//!
//! A [`CredentialIssuer`] is made only after a PROBE credential, signed by the signer it was given under
//! the certificate it was given, verified under the anchors at the given time: a certificate that does
//! not verify, a window that does not cover the moment, anchors that do not know the identity key, or a
//! signer whose public key is not the certificate's `cred_pub` all fail the probe, and NO issuer value
//! exists. So a key that cannot issue a credential every node accepts is never put to use: S9f keeps the
//! old key and shows the failure.
//!
//! `issue` takes the claims WITHOUT the key epoch and stamps it from the certificate: a caller cannot
//! pass another. It refuses what every verifier would refuse (the shape rules, a lifetime above the
//! rules' maximum, a credential that would outlive its certificate), and checks the signature the
//! holder returned against the holder's own public key before the credential leaves.

use azlin_keys::Span;
use azlin_regbase::{
    AccountId, AnchorSet, CredentialNonce, KeyEpoch, LeaderTime, MemberId, PolicyEpoch,
};

use crate::canon::{domain_of, Canonical};
use crate::cert::Certificate;
use crate::claims::{CredentialClaims, GrantClaims};
use crate::credential::{verify_credential, Credential};
use crate::error::{CredError, IssueError, IssuerError, ProbeFailure};
use crate::scopes::{LogRef, Scope, Scopes};
use crate::signer::{sign_checked, SignFailure, Signer};
use crate::verify::{crossing, VerifierNow, VerifyRules};

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
