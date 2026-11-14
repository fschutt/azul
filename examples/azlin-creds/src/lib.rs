//! Azlin credentials: the signed structures of the session plane, with issue and verify, and nothing about
//! the registry's state (planning/architecture.md section 2.4 "Credentials are verifiable on every
//! node", section 5.11 "Access: entitlements, credentials, assertions, attestations", section 2.5
//! "Signed structures, each with its own domain string"; build unit S9, lane W1-S9e).
//!
//! | module | what |
//! |---|---|
//! | [`canon`] | the four signature domains, the [`Canonical`] seam (the bytes that are signed), the codec's limits |
//! | [`cert`] | [`Certificate`]: the identity key certifies a credential key; `certify`, `verify_cert` |
//! | [`claims`] | [`CredentialClaims`], [`GrantClaims`], [`AssertionClaims`], [`SsoClaim`] and their wire form |
//! | [`credential`] | [`Credential`], [`VerifiedCredential`], `verify_credential` |
//! | [`assertion`] | [`Assertion`], [`VerifiedAssertion`], `verify_assertion`: the member assertion services verify |
//! | [`entitlement`] | [`EntitlementClaims`], [`SignedEntitlement`], `sign_entitlement`, `verify_entitlement` |
//! | [`scopes`] | [`Scopes`]: what a credential may do at most (only narrows) |
//! | [`signer`] | [`Signer`], the seam of the key holder; [`SeedSigner`] |
//! | [`verify`] | [`VerifierNow`], [`ClockTrust`], [`VerifyRules`]: who judges time (D-12) |
//! | [`issuer`] | [`CredentialIssuer`], [`AssertionIssuer`]: made only after a probe verified (section 1.4 rule 9) |
//! | [`error`] | the typed refusals |
//!
//! THE SIGNING RULE: a signature covers `domain || 0x00 || canonical CBOR`, framed once by azlin-keys'
//! `signing_input`; a [`Signer`] takes the domain (a constant, a `&Domain`, never a string) and the
//! message and frames inside, so no input is framed twice. The bytes are written and read with
//! `azlin-cbor`; a signed structure keeps the exact bytes its signature covers, so a field a newer
//! issuer adds does not break an older verifier (a reader ignores a key it does not know and refuses one
//! `crit` lists).
//!
//! THE CHAIN (section 2.4): the metadata group's long-lived IDENTITY key (named by the cluster map and
//! carried in the [`AnchorSet`](azlin_regbase::AnchorSet)) certifies each CREDENTIAL key, and every
//! credential carries that certificate. A node verifies a credential with two signature checks and no
//! registry read; whether the credential is revoked is the registry's question, asked by S9g.
//!
//! It is the crate the services link: pure (no clock, no file, no network, no randomness of its own: all
//! come in through seams), no unsafe code, and it builds for `wasm32-unknown-unknown`.

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

pub mod assertion;
pub mod canon;
pub mod cert;
pub mod claims;
pub mod credential;
pub mod entitlement;
pub mod error;
pub mod issuer;
pub mod scopes;
pub mod signer;
pub mod verify;

pub use assertion::{
    verify_assertion, verify_assertion_bytes, verify_assertion_with, Assertion, VerifiedAssertion,
};
pub use canon::{
    signing_preimage, Canonical, ALL_DOMAINS, CREDENTIAL_CERT_V1, CREDENTIAL_V2, ENTITLEMENT_V2,
    MEMBER_ASSERT_V1,
};
pub use cert::{certify, verify_cert, Certificate, MAX_CERT_SPAN};
pub use claims::{AssertionClaims, CredentialClaims, GrantClaims, SsoClaim, MAX_GROUPS};
pub use credential::{
    verify_credential, verify_credential_bytes, verify_credential_with, Credential,
    VerifiedCredential,
};
pub use entitlement::{
    sign_entitlement, verify_entitlement, verify_entitlement_with, EntitlementClaims,
    SignedEntitlement, VerifiedEntitlement, ISSUE_SKEW,
};
pub use error::{
    AssertError, CanonicalError, CertError, CredError, EntitlementError, IssueError, IssuerError,
    ProbeFailure, WireError,
};
pub use issuer::{AssertionIssuer, CredentialIssuer};
pub use scopes::{
    AppPerms, AppScope, LinkRef, LogRef, RequestRef, Scope, ScopedOp, Scopes, ScopesError,
    MAX_SCOPES,
};
pub use signer::{SeedSigner, SignError, Signer};
pub use verify::{ClockTrust, NodeTime, VerifierNow, VerifyRules};
