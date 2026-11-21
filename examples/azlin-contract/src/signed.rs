//! The signed structures and their domain strings (planning/architecture.md 2.5 "Signed structures, each
//! with its own domain string"). A signature is made over `domain || 0x00 || the structure's bytes`, so a
//! signature of one structure is no signature of another; the construction itself is the CBOR codec's and
//! the signing helpers' (azlin-cbor, azlin-keys). This is the list.

/// Where a domain string comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DomainSource {
    /// Written in 2.5's list.
    Plan,
    /// Proposed by the units that build the structure, because the plan gives it a signature and no domain
    /// (X-DELTA-W1S5-9): it moves to `Plan` when 2.5's list names it.
    Proposed,
}

/// One signed structure.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SignedDomain {
    pub domain: &'static str,
    /// What the structure is, and where the plan describes it.
    pub structure: &'static str,
    pub source: DomainSource,
}

const fn plan(domain: &'static str, structure: &'static str) -> SignedDomain {
    SignedDomain {
        domain,
        structure,
        source: DomainSource::Plan,
    }
}

const fn proposed(domain: &'static str, structure: &'static str) -> SignedDomain {
    SignedDomain {
        domain,
        structure,
        source: DomainSource::Proposed,
    }
}

/// 2.5's list, in its order, then the proposals. "...plus the map, delegation and seed-list formats" are
/// not strings here: the plan names those formats without a domain (X-DELTA-W1S5-5).
pub const SIGNED_DOMAINS: &[SignedDomain] = &[
    plan(
        "azlin-entitlement-v2",
        "entitlement v2 (5.11), signed by the payments key",
    ),
    plan("azlin-team-log-v1", "a team log entry (5.9)"),
    plan("azlin-space-chain-v1", "a space chain entry (5.9)"),
    plan(
        "azlin-sso-attest-v1",
        "an SSO attestation (5.11), signed by the identity service",
    ),
    plan(
        "azlin-member-assert-v1",
        "a member assertion (5.11), signed by the metadata group",
    ),
    plan("azlin-audit-seg-v1", "an audit segment (5.14)"),
    plan(
        "azlin-credential-v2",
        "a credential (5.11), signed by the metadata group's credential key",
    ),
    proposed(
        "azlin-envelope-v2",
        "an envelope's `sig` (5.5): `signer`, `sig`",
    ),
    proposed(
        "azlin-pointer-v2",
        "a pointer's `sig` (5.7): `author_device`, `sig`",
    ),
    proposed(
        "azlin-log-record-v1",
        "an append-log record's signature (5.13): `author_device`",
    ),
];
