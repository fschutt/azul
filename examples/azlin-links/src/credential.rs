//! The link credential (planning/architecture.md section 5.12 "Resolving": "a **link credential**:
//! short-lived (15 minutes, renewed while the link allows it) and limited to the link's grants; plus
//! `request_upload` to the paired request and `log_append` to the comment log, where those are on;
//! everything after resolving uses it, including tier 2 of the gate through a relay (3.7)"), and the
//! rule of section 5.11 that "`scopes` only narrows".
//!
//! This module makes the SPEC of a credential (`CredentialSpec`: how long it lives and what it may
//! do); the node signs it into a credential v2 with the metadata group's credential key (S9e), and
//! reads the signed claims back as `CredentialClaims` when the holder comes to renew it or to read.
//!
//! - **15 minutes** (`CREDENTIAL_TTL`), and never longer than the link itself: the lifetime is
//!   cut at the earliest time the link ends by a date (`expires_at`, `expires_after_open`,
//!   `idle_expiry`), so a credential never outlives the link's own deadline;
//! - **scopes**: `Read(link)` always, `RequestUpload(request)` only where `allow_upload` is on,
//!   `LogAppend(log)` only where `comment` is on. `Read` names the LINK, not a list of objects: a
//!   link may name 16,384 objects, and "nodes check grants locally, so no credential has to list"
//!   them (5.11). What the link may read is `may_read`: the grants of the record, checked on every
//!   read, so a re-pointed live folder is served at once and a rotated link serves nothing to its
//!   old credentials;
//! - **renewal** (`renew`) re-checks every rule `resolve` checks, counts nothing, never adds a scope
//!   the old credential did not have, and refuses a credential of another key (a rotation), one that
//!   has expired, or one the link no longer answers to. It does not ask for the password again: the
//!   old credential could only be issued after the login, and a password change is a rotation
//!   (S16c1: `password_change_requires_rotation`), which ends the old credential's id.
//!
//! Implements planning/architecture.md section 5.12 ("Resolving") and section 5.11 (credential v2:
//! "scopes only narrows").

use azlin_seams::{Span, Stamp};

use crate::id::PublicLinkId;
use crate::record::{ByteRange, Grants, LinkRecord, LogId, ObjectRef, RequestId};
use crate::resolve::{check_open, Opening, ResolveCtx, ResolveRefusal};

/// How long a link credential lives: 15 minutes ("short-lived (15 minutes, renewed while the link
/// allows it)").
///
/// Implements planning/architecture.md section 5.12 ("Resolving").
///
/// Caller: `issue` (through `resolve` and `renew`).
pub const CREDENTIAL_TTL: Span = Span::minutes(15);

/// One thing a link credential may do. The scopes only narrow what the holder could do (section
/// 5.11); the credential of a link carries `Read` and, where the options turn them on, the other two.
///
/// Implements planning/architecture.md section 5.12 ("Resolving": "`request_upload` to the paired
/// request and `log_append` to the comment log, where those are on") and section 5.11.
///
/// Caller: `CredentialSpec`, `CredentialClaims`, `renew`; Seam: S9e (the credential v2 lists them as
/// its `scopes`), LINKS-NODE (`object_get`, `request_upload` and `log_append` check them).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum LinkScope {
    /// Read what the link's grants name (`may_read`): the scope names the link.
    Read(PublicLinkId),
    /// Upload through the link's paired request (`allow_upload`).
    RequestUpload(RequestId),
    /// Append to the link's comment log (`comment`).
    LogAppend(LogId),
}

/// What a credential to be issued is: how long it lives and what it may do. The node signs it.
///
/// Implements planning/architecture.md section 5.12 ("Resolving") and section 5.11 (credential v2).
///
/// Caller: `resolve` (`Resolved::credential`), `renew`; Seam: LINKS-NODE (it signs the spec with
/// S9e's credential key and puts `scopes` into the credential v2).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialSpec {
    /// How long the credential lives from the moment it is issued: at most `CREDENTIAL_TTL`.
    pub ttl: Span,
    /// What it may do: `Read` of the link, and the optional two.
    pub scopes: Vec<LinkScope>,
}

impl CredentialSpec {
    /// Whether the credential reads the link `link`.
    pub fn reads(&self, link: &PublicLinkId) -> bool {
        self.scopes.contains(&LinkScope::Read(*link))
    }

    /// The time the credential ends when it is issued at `issued_at`; `None` when that is beyond the
    /// largest stamp.
    pub fn expires_at(&self, issued_at: Stamp) -> Option<Stamp> {
        issued_at.checked_add(self.ttl)
    }
}

/// The claims of a link credential the node has already verified (signature, key epoch): when it
/// was issued, when it ends and what it may do. A credential is only as old as its claims say.
///
/// Implements planning/architecture.md section 5.11 (credential v2: `issued_at`, `expires_at`,
/// `scopes`) and section 5.12 ("renewed while the link allows it").
///
/// Caller: `renew`, `may_read`; Seam: LINKS-NODE (it builds the claims from the credential the
/// holder presents, after S9e has verified it).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CredentialClaims {
    issued_at: Stamp,
    expires_at: Stamp,
    scopes: Vec<LinkScope>,
}

impl CredentialClaims {
    /// The claims of a credential issued at `issued_at` that ends at `expires_at` with `scopes`.
    pub fn new(issued_at: Stamp, expires_at: Stamp, scopes: Vec<LinkScope>) -> CredentialClaims {
        CredentialClaims {
            issued_at,
            expires_at,
            scopes,
        }
    }

    /// The claims of the credential that `spec` describes when it is issued at `issued_at`; `None`
    /// when its end is beyond the largest stamp.
    pub fn from_spec(spec: &CredentialSpec, issued_at: Stamp) -> Option<CredentialClaims> {
        let expires_at = spec.expires_at(issued_at)?;
        Some(CredentialClaims {
            issued_at,
            expires_at,
            scopes: spec.scopes.clone(),
        })
    }

    /// When the credential was issued.
    pub fn issued_at(&self) -> Stamp {
        self.issued_at
    }

    /// When the credential ends.
    pub fn expires_at(&self) -> Stamp {
        self.expires_at
    }

    /// What the credential may do.
    pub fn scopes(&self) -> &[LinkScope] {
        &self.scopes
    }

    /// Whether the credential reads the link `link`.
    pub fn reads(&self, link: &PublicLinkId) -> bool {
        self.scopes.contains(&LinkScope::Read(*link))
    }
}

/// How long a credential issued at `now` lives: `CREDENTIAL_TTL`, cut at the link's earliest date
/// (`deadline`). A deadline that is not after `now` gives no time at all (the rules refuse such a
/// link before a credential is made, so this arm is a belt for the brace).
fn ttl_for(deadline: Option<Stamp>, now: Stamp) -> Span {
    if azlin_seams::mutant!("links_credential_ttl_unbounded") {
        return Span::days(3650);
    }
    match deadline {
        None => CREDENTIAL_TTL,
        Some(end) => match end.elapsed_since(now) {
            Some(left) if left < CREDENTIAL_TTL => left,
            Some(_) => CREDENTIAL_TTL,
            None => Span::ZERO,
        },
    }
}

/// The credential a link that passed every rule gives at `now`: 15 minutes at most and not past the
/// link's own deadline, with the scopes the options turn on.
pub(crate) fn issue(r: &LinkRecord, opening: &Opening, now: Stamp) -> CredentialSpec {
    let mut scopes = vec![LinkScope::Read(*r.id())];
    if r.options().allow_upload {
        if let Some(request) = r.request_id() {
            scopes.push(LinkScope::RequestUpload(*request));
        }
    }
    if r.options().comment {
        if let Some(log) = r.comment_log() {
            scopes.push(LinkScope::LogAppend(*log));
        }
    }
    CredentialSpec {
        ttl: ttl_for(opening.deadline, now),
        scopes,
    }
}

/// Whether `claims` still belong to the link as it is now: it reads the id the link answers to, was
/// issued since the link's key began, and has not ended.
fn claims_fit(r: &LinkRecord, claims: &CredentialClaims, now: Stamp) -> bool {
    claims.reads(r.id()) && claims.issued_at >= r.keyed_at() && now < claims.expires_at
}

/// Renews a credential: the link is checked again against EVERY rule of `resolve` (the audience, the
/// dates, the counts, the devices, `paused`, the blocks, the daily cap), nothing is counted, and the
/// new credential has the scopes the old one had and the link still allows, never one more. A
/// credential of another key (the link was rotated), one that has ended, or one that does not read this
/// link is `StaleCredential`: the holder resolves the link again. A password is not asked for again.
///
/// Implements planning/architecture.md section 5.12 ("Resolving": "renewed while the link allows
/// it") and section 5.11 ("scopes only narrows").
///
/// Seam: LINKS-NODE (the credential refresh of a link credential: it verifies the signature,
/// builds the claims and calls this with the context of the renewing request).
pub fn renew(
    r: &LinkRecord,
    old: &CredentialClaims,
    c: &ResolveCtx<'_>,
) -> Result<CredentialSpec, ResolveRefusal> {
    if !claims_fit(r, old, c.now) && !azlin_seams::mutant!("links_stale_credential_renewed") {
        return Err(ResolveRefusal::StaleCredential);
    }
    let opening = if azlin_seams::mutant!("links_renewal_skips_rules") {
        Opening { deadline: None }
    } else {
        check_open(r, c, true)?
    };
    let full = issue(r, &opening, c.now);
    let scopes = if azlin_seams::mutant!("links_scopes_widen_on_renewal") {
        full.scopes
    } else {
        full.scopes
            .into_iter()
            .filter(|scope| old.scopes.contains(scope))
            .collect()
    };
    Ok(CredentialSpec {
        ttl: full.ttl,
        scopes,
    })
}

/// Whether the grants cover the bytes asked for: a whole object is covered by a grant of the whole
/// object; a range is covered by a grant of the whole object or by granted ranges of the object that
/// leave no gap in it (adjacent ranges together cover a range that spans them). Anything else, and
/// every object no grant names, is not covered ("nothing else in a pack is served").
fn grants_cover(grants: &Grants, object: &ObjectRef, wanted: Option<ByteRange>) -> bool {
    let mut granted: Vec<(u64, u64)> = Vec::new();
    for grant in grants.as_slice() {
        if grant.object != *object {
            continue;
        }
        match grant.range {
            None => return true,
            Some(range) => granted.push((range.start().get(), range.end().get())),
        }
    }
    let Some(range) = wanted else {
        return false;
    };
    let mut covered_to = range.start().get();
    let end = range.end().get();
    granted.sort_unstable();
    for (start, stop) in granted {
        if start > covered_to {
            return false;
        }
        if stop > covered_to {
            covered_to = stop;
        }
        if covered_to >= end {
            return true;
        }
    }
    false
}

/// Whether the credential `claims` may read `range` of `object` (the whole object when `range` is
/// `None`) of the link `r` at `now`: the credential reads this link, belongs to its current key and
/// has not ended, and the bytes are the link's own manifest object (`resolve` hands it out: the
/// recipient must be able to read it) or are covered by the link's grants. The link's own state
/// (revoked, paused, expired, the caps) is `admit_bytes`'s, which the node asks as well, for every
/// read.
///
/// Implements planning/architecture.md section 5.12 ("limited to the link's grants"; `grants`: "the
/// objects and byte ranges it may read; nothing else in a pack is served").
///
/// Seam: LINKS-NODE (`object_get` with a link credential).
pub fn may_read(
    r: &LinkRecord,
    claims: &CredentialClaims,
    now: Stamp,
    object: &ObjectRef,
    range: Option<ByteRange>,
) -> bool {
    if !claims_fit(r, claims, now) {
        return false;
    }
    if azlin_seams::mutant!("links_grants_ignored") || object == r.manifest_object() {
        return true;
    }
    grants_cover(r.grants(), object, range)
}
