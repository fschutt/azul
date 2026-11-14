//! Entitlement v2: "`azlin-entitlement-v2`, signed by the payments key, submitted with `entitlement_put`:
//! `account`, `kind`, `seats`, `quota_bytes`, `retention_days`, `features` (team, sso, lock, audit,
//! residency_choice, escrow, requests...), `period_until`, `state`, `regions_allowed`, `issued_at`"
//! (planning/architecture.md section 5.11; section 2.4: "Entitlements come in signed: the payments
//! service ... submits `azlin-entitlement-v2` to the account's metadata group through the protocol. The
//! group verifies the payments key and applies it to `reg_account`").
//!
//! [`EntitlementClaims`] is the ONE canonical signed entitlement (COMMON-W1R R10): S13a's
//! `EntitlementDraft` keeps exactly its fields, the state words are `Active` and `Lapsed`, `version`
//! is a number that grows per account (the higher wins; DRIFT D-9). The payments service signs it
//! (the group-signed `free` path is not built: X-D-11), a metadata group verifies it with the payments
//! key's windows of the anchors.
//!
//! THE KEY WINDOW: "entitlements signed by the old key stay valid until they expire" (section 2.4, and
//! COMMANDS2 `keys rotate payments`): the key's window is checked at the day the entitlement was ISSUED,
//! not at the moment of verification, so a rotation does not kill the entitlements of the old key (and
//! the outbox's re-submission after a restore keeps working). Two guards keep that rule from being a hole:
//! the entitlement must not be issued on a day that has not begun (a key staged for a future window
//! cannot sign into it), and a key the registry has REVOKED is refused from the day of its revocation on:
//! `KeyWindow::revoked_from` (DRIFT X-DELTA-W1S9E-5, decision D-W1S9E3; X-D-11 asks to tell the
//! RETIREMENT of a key from its REVOCATION) makes `verify_entitlement` answer `Revoked`, an ALARM, when
//! the day the entitlement was issued is at or after it, whatever the window of the key says. THE LIMIT:
//! an entitlement a thief dates BEFORE `revoked_from` cannot be told from the old, honest one, so the
//! registry fills `revoked_from` with the earliest second the key may have been in other hands (the
//! window's `not_before` when that is unknown: every date of the window is then refused).
//!
//! The wire form is a map: key 1 the payload (the canonical claims), key 2 the payments public key, key
//! 3 the signature. "The bytes the registry stores are the claims' bytes and the signature": the
//! registry keeps `SignedEntitlement::encode` verbatim.

use core::fmt;

use azlin_cbor::{decode, IntMap, IntMapBuilder, Value};
use azlin_keys::Span;
use azlin_regbase::{
    AccountId, AccountKind, AnchorSet, BoundError, DayStart, EntitlementState, Features,
    LeaderTime, PublicKey32, RegionId, RegionSet, Signature64, SmallText,
};
use azlin_seams::{ByteCount, Ed25519Verifier, SigAlg, SignatureVerifier};

use crate::canon::{
    build, bytes_of, domain_of, preimage_of, read_u32, text_of, uint_of, write, Canonical, Short,
    ENTITLEMENT_V2, OUTER_LIMITS, PAYLOAD_LIMITS,
};
use crate::error::{CanonicalError, EntitlementError, WireError};
use crate::signer::{sign_checked, SignFailure, Signer};

/// The wire keys of an entitlement's claims.
const E_ACCOUNT: u64 = 1;
const E_KIND: u64 = 2;
const E_SEATS: u64 = 3;
const E_QUOTA: u64 = 4;
const E_RETENTION: u64 = 5;
const E_FEATURES: u64 = 6;
const E_PERIOD: u64 = 7;
const E_STATE: u64 = 8;
const E_REGIONS: u64 = 9;
const E_ISSUED: u64 = 10;
const E_VERSION: u64 = 11;

/// The wire keys of the wrapper.
const W_PAYLOAD: u64 = 1;
const W_KEY: u64 = 2;
const W_SIG: u64 = 3;

/// How far ahead of the leader's stamp the day of an entitlement may lie: the payments machine's clock
/// may run ahead of the leader's by a few seconds at midnight. (DRIFT D-12: the same 30 s as the skew of
/// credentials.)
pub const ISSUE_SKEW: Span = Span::secs(30);

/// One day in seconds: a retention is a whole number of days on the wire.
const DAY_SPAN: Span = Span::days(1);

/// The claims of an entitlement v2.
///
/// Implements planning/architecture.md section 5.11 (the field list), COMMON-W1R R10 and DRIFT D-9
/// (`version` and `state` are proposals).
///
/// Caller: S13a (`EntitlementDraft` is this, field for field; at N24 `sign` calls `sign_entitlement`),
/// S9d (the entitlement path reads the verified claims), S9i (`CredsEntitlementVerifier`).
#[derive(Clone, PartialEq, Eq)]
pub struct EntitlementClaims {
    /// The account.
    pub account: AccountId,
    /// The kind of account the entitlement is for.
    pub kind: AccountKind,
    /// How many seats (members) the account has.
    pub seats: u32,
    /// The storage quota.
    pub quota: ByteCount,
    /// How long deleted data is kept: a whole number of days.
    pub retention: Span,
    /// The features: team, sso, lock, audit, residency_choice, escrow, requests, ... (words this build
    /// does not know are kept).
    pub features: Features,
    /// The end of the paid period; `None` for an entitlement without one.
    pub period_until: Option<DayStart>,
    /// Paid up, or lapsed.
    pub state: EntitlementState,
    /// The regions the account may choose.
    pub regions_allowed: RegionSet,
    /// The day the entitlement was issued (a day start: D37).
    pub issued_at: DayStart,
    /// A number that grows with every entitlement of the account: the higher wins.
    pub version: u64,
}

/// Prints the account as four bytes and `..`: never the whole id.
impl fmt::Debug for EntitlementClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("EntitlementClaims")
            .field("account", &Short(self.account.as_bytes()))
            .field("kind", &self.kind)
            .field("seats", &self.seats)
            .field("quota", &self.quota)
            .field("retention", &self.retention)
            .field("features", &self.features)
            .field("period_until", &self.period_until)
            .field("state", &self.state)
            .field("regions_allowed", &self.regions_allowed)
            .field("issued_at", &self.issued_at)
            .field("version", &self.version)
            .finish()
    }
}

/// The days of a retention, or why it is not a whole number of them.
fn retention_days(retention: Span) -> Result<u64, CanonicalError> {
    let whole = DAY_SPAN.as_secs();
    if !retention.as_secs().is_multiple_of(whole) {
        return Err(CanonicalError::NotWholeDays);
    }
    let days = retention.as_secs() / whole;
    if u32::try_from(days).is_err() {
        return Err(CanonicalError::TooLarge);
    }
    Ok(days)
}

/// What a bounded type refused, as a decode refusal.
fn bound_refusal(e: BoundError) -> WireError {
    match e {
        BoundError::TooMany { .. } => WireError::TooMany,
        BoundError::TooLong { .. } | BoundError::Empty { .. } | BoundError::BadChar { .. } => {
            WireError::Malformed
        }
    }
}

impl EntitlementClaims {
    fn to_value(&self) -> Result<Value, CanonicalError> {
        let days = retention_days(self.retention)?;
        let features = Value::Array(
            self.features
                .as_slice()
                .iter()
                .map(|word| text_of(word.as_str()))
                .collect(),
        );
        let regions = Value::Array(
            self.regions_allowed
                .as_slice()
                .iter()
                .map(|region| uint_of(u64::from(region.get())))
                .collect(),
        );
        let mut b = IntMapBuilder::new()
            .put(E_ACCOUNT, bytes_of(self.account.as_bytes()))
            .put(E_KIND, text_of(self.kind.as_str()))
            .put(E_SEATS, uint_of(u64::from(self.seats)))
            .put(E_QUOTA, uint_of(self.quota.get()))
            .put(E_RETENTION, uint_of(days))
            .put(E_FEATURES, features)
            .put(E_STATE, text_of(self.state.as_str()))
            .put(E_REGIONS, regions)
            .put(E_ISSUED, uint_of(self.issued_at.start().as_secs()))
            .put(E_VERSION, uint_of(self.version));
        if let Some(until) = self.period_until {
            b = b.put(E_PERIOD, uint_of(until.start().as_secs()));
        }
        build(b)
    }

    /// The claims of the signed bytes (the payload of an entitlement), STRICTLY: a missing key, a wrong
    /// type, a word outside its vocabulary, a time that is not a day start, a set that is not in its one
    /// canonical form (sorted, without duplicates) are refused; a key nobody asked for and `crit` does
    /// not list is ignored.
    ///
    /// Caller: `SignedEntitlement::decode`, `verify_entitlement`.
    pub fn decode_payload(bytes: &[u8]) -> Result<EntitlementClaims, WireError> {
        let value = decode(bytes, &PAYLOAD_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        let account = AccountId::from_bytes(map.req_bytes_n::<16>(E_ACCOUNT)?);
        let kind = map
            .req_text(E_KIND)?
            .parse::<AccountKind>()
            .map_err(|_| WireError::UnknownWord)?;
        let seats = read_u32(&mut map, E_SEATS)?;
        let quota = ByteCount::new(map.req_u64(E_QUOTA)?);
        let days = map.req_u64(E_RETENTION)?;
        let days = u32::try_from(days).map_err(|_| WireError::OutOfRange(E_RETENTION))?;
        let retention = Span::days(days);

        let mut names: Vec<SmallText> = Vec::new();
        for item in map.req_array(E_FEATURES)? {
            let Value::Text(text) = item else {
                return Err(WireError::Malformed);
            };
            names.push(SmallText::new(text).map_err(|_| WireError::Malformed)?);
        }
        let features = Features::new(names.clone()).map_err(bound_refusal)?;
        if features.as_slice() != names.as_slice() {
            return Err(WireError::NotCanonical);
        }

        let period_until = match map.opt_u64(E_PERIOD)? {
            Some(number) => Some(day_start(number, E_PERIOD)?),
            None => None,
        };
        let state = map
            .req_text(E_STATE)?
            .parse::<EntitlementState>()
            .map_err(|_| WireError::UnknownWord)?;

        let mut regions: Vec<RegionId> = Vec::new();
        for item in map.req_array(E_REGIONS)? {
            let Value::Uint(number) = item else {
                return Err(WireError::Malformed);
            };
            let number = u16::try_from(*number).map_err(|_| WireError::OutOfRange(E_REGIONS))?;
            regions.push(RegionId::new(number));
        }
        let regions_allowed = RegionSet::new(regions.clone()).map_err(bound_refusal)?;
        if regions_allowed.as_slice() != regions.as_slice() {
            return Err(WireError::NotCanonical);
        }

        let issued_at = day_start(map.req_u64(E_ISSUED)?, E_ISSUED)?;
        let version = map.req_u64(E_VERSION)?;
        map.finish()?;
        Ok(EntitlementClaims {
            account,
            kind,
            seats,
            quota,
            retention,
            features,
            period_until,
            state,
            regions_allowed,
            issued_at,
            version,
        })
    }
}

/// A day start read from a wire number: a time that is not a multiple of a day is refused, never
/// rounded.
fn day_start(number: u64, key: u64) -> Result<DayStart, WireError> {
    DayStart::from_start(LeaderTime::from_secs(number)).ok_or(WireError::OutOfRange(key))
}

impl Canonical for EntitlementClaims {
    const DOMAIN: azlin_regbase::Domain = ENTITLEMENT_V2;

    fn canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        write(&self.to_value()?, &PAYLOAD_LIMITS)
    }
}

/// An entitlement as the registry keeps it: the claims' bytes, the payments public key that signed them,
/// the signature.
///
/// The claims it shows are UNVERIFIED: only [`verify_entitlement`] turns it into a
/// [`VerifiedEntitlement`].
///
/// Implements planning/architecture.md section 5.11 and section 2.4.
///
/// Caller: `sign_entitlement` (S13a at N24), S9d and S9i (the bytes of `entitlement_put`, stored
/// verbatim in `reg_account.entitlement`), `verify_entitlement`.
#[derive(Clone, PartialEq, Eq)]
pub struct SignedEntitlement {
    claims: EntitlementClaims,
    key: PublicKey32,
    sig: Signature64,
    payload: Vec<u8>,
}

impl SignedEntitlement {
    /// The signed entitlement of these claims, this key and this signature; the payload is the canonical
    /// bytes of `claims`. Whether `sig` covers them is the verifier's question.
    ///
    /// Caller: the tests.
    pub fn from_parts(
        claims: EntitlementClaims,
        key: PublicKey32,
        sig: Signature64,
    ) -> Result<SignedEntitlement, CanonicalError> {
        let payload = claims.canonical_bytes()?;
        Ok(SignedEntitlement {
            claims,
            key,
            sig,
            payload,
        })
    }

    /// The claims, UNVERIFIED.
    pub fn claims(&self) -> &EntitlementClaims {
        &self.claims
    }

    /// The payments public key that signed.
    pub fn key(&self) -> &PublicKey32 {
        &self.key
    }

    /// The signature.
    pub fn signature(&self) -> &Signature64 {
        &self.sig
    }

    /// The bytes the signature covers (the canonical claims).
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// The canonical bytes of the whole entitlement: what the registry stores in `reg_account`.
    ///
    /// Caller: S13a (the bytes the payments service submits), S9d and S9i (stored verbatim).
    pub fn encode(&self) -> Result<Vec<u8>, CanonicalError> {
        let value = build(
            IntMapBuilder::new()
                .put(W_PAYLOAD, bytes_of(&self.payload))
                .put(W_KEY, bytes_of(self.key.as_bytes()))
                .put(W_SIG, bytes_of(self.sig.as_bytes())),
        )?;
        write(&value, &OUTER_LIMITS)
    }

    /// The signed entitlement of canonical bytes, STRICTLY. A key of the wrapper or of the claims that
    /// nobody knows is ignored unless `crit` lists it.
    ///
    /// Caller: S9i's `CredsEntitlementVerifier` (the bytes of a `SignedBytes`), the tests.
    pub fn decode(bytes: &[u8]) -> Result<SignedEntitlement, WireError> {
        let value = decode(bytes, &OUTER_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        let payload = map.req_bytes(W_PAYLOAD)?;
        let key = PublicKey32::from_bytes(map.req_bytes_n::<32>(W_KEY)?);
        let sig = Signature64::from_bytes(map.req_bytes_n::<64>(W_SIG)?);
        map.finish()?;
        let claims = EntitlementClaims::decode_payload(payload)?;
        Ok(SignedEntitlement {
            claims,
            key,
            sig,
            payload: payload.to_vec(),
        })
    }
}

/// Prints the account as four bytes, the key as four bytes and the payload's length: never a signature
/// byte.
impl fmt::Debug for SignedEntitlement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SignedEntitlement")
            .field("account", &Short(self.claims.account.as_bytes()))
            .field("version", &self.claims.version)
            .field("key", &Short(self.key.as_bytes()))
            .field("payload_len", &self.payload.len())
            .finish()
    }
}

/// An entitlement that `verify_entitlement` accepted: its claims, read from the signed bytes, and the
/// payments key that signed them.
///
/// A witness. It has no public constructor, no `Clone`, no `Default`, no `Deserialize` and no
/// `From<&str>`:
///
/// ```compile_fail,E0599
/// // no `Clone`
/// fn copy(v: azlin_creds::VerifiedEntitlement) {
///     let _ = v.clone();
/// }
/// ```
///
/// ```compile_fail,E0599
/// // no `Default`
/// let _ = azlin_creds::VerifiedEntitlement::default();
/// ```
///
/// ```compile_fail,E0624
/// // no public constructor
/// let _ = azlin_creds::VerifiedEntitlement::new(todo!(), todo!());
/// ```
///
/// ```compile_fail,E0277
/// // no `From<&str>`
/// let _: azlin_creds::VerifiedEntitlement = "an entitlement".into();
/// ```
///
/// The control compiles:
///
/// ```
/// fn read(v: &azlin_creds::VerifiedEntitlement) -> u64 {
///     v.version()
/// }
/// ```
///
/// Implements planning/architecture.md section 2.4 ("The group verifies the payments key and applies it
/// to `reg_account`").
///
/// Caller: S9d (the entitlement path applies it), S9i's `CredsEntitlementVerifier`.
pub struct VerifiedEntitlement {
    claims: EntitlementClaims,
    key: PublicKey32,
}

impl VerifiedEntitlement {
    /// Made only by a verification.
    pub(crate) fn new(claims: EntitlementClaims, key: PublicKey32) -> VerifiedEntitlement {
        VerifiedEntitlement { claims, key }
    }

    /// The account.
    pub fn account(&self) -> AccountId {
        self.claims.account
    }

    /// The kind of account.
    pub fn kind(&self) -> AccountKind {
        self.claims.kind
    }

    /// The seats.
    pub fn seats(&self) -> u32 {
        self.claims.seats
    }

    /// The storage quota.
    pub fn quota(&self) -> ByteCount {
        self.claims.quota
    }

    /// The retention, a whole number of days.
    pub fn retention(&self) -> Span {
        self.claims.retention
    }

    /// The features.
    pub fn features(&self) -> &Features {
        &self.claims.features
    }

    /// The end of the paid period.
    pub fn period_until(&self) -> Option<DayStart> {
        self.claims.period_until
    }

    /// Paid up, or lapsed.
    pub fn state(&self) -> EntitlementState {
        self.claims.state
    }

    /// The regions the account may choose.
    pub fn regions_allowed(&self) -> &RegionSet {
        &self.claims.regions_allowed
    }

    /// The day the entitlement was issued.
    pub fn issued_at(&self) -> DayStart {
        self.claims.issued_at
    }

    /// The version: the higher one wins.
    pub fn version(&self) -> u64 {
        self.claims.version
    }

    /// The payments key that signed it: a key of the anchors whose window covers the day of issue and
    /// that is not revoked from that day on (`KeyWindow::revoked_from`, DRIFT X-DELTA-W1S9E-5).
    pub fn key(&self) -> &PublicKey32 {
        &self.key
    }

    /// All the claims.
    pub fn claims(&self) -> &EntitlementClaims {
        &self.claims
    }
}

impl fmt::Debug for VerifiedEntitlement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("VerifiedEntitlement")
            .field("account", &Short(self.claims.account.as_bytes()))
            .field("version", &self.claims.version)
            .field("state", &self.claims.state)
            .field("key", &Short(self.key.as_bytes()))
            .finish()
    }
}

impl From<SignFailure> for EntitlementError {
    fn from(f: SignFailure) -> EntitlementError {
        match f {
            SignFailure::Sign(e) => EntitlementError::Sign(e),
            SignFailure::Fault => EntitlementError::SignerFault,
        }
    }
}

/// The entitlement of these claims, signed by the payments key behind `payments` under
/// `azlin-entitlement-v2`; the signature is checked against the signer's own public key before the
/// entitlement is returned. Refused: a retention that is not a whole number of days, claims that do not
/// fit the codec's limits (`Malformed`), a holder that did not sign (`Sign`) or whose signature does not
/// verify (`SignerFault`).
///
/// Implements planning/architecture.md section 5.11 and section 2.4 (COMMON-W1R R10: at N24 S13a's
/// `sign` calls this).
///
/// Caller: S13a (`EntitlementDraft::sign` at N24, through its `KeySigner`), AzCtl's `invite` (the same
/// call), the tests.
pub fn sign_entitlement(
    payments: &dyn Signer,
    claims: EntitlementClaims,
) -> Result<SignedEntitlement, EntitlementError> {
    let payload = claims.canonical_bytes()?;
    let sig = sign_checked(payments, &domain_of::<EntitlementClaims>(), &payload)?;
    Ok(SignedEntitlement {
        claims,
        key: payments.public(),
        sig,
        payload,
    })
}

/// Whether the entitlement is one the anchors' payments keys vouch for, at the leader's stamp `now`.
///
/// What it checks, in this order, and the first failure is the answer: the signing key is a payments key
/// of the anchors (`UnknownKey`; an empty anchor set knows none, and refuses); one of its windows covers
/// the DAY THE ENTITLEMENT WAS ISSUED (`KeyWindow`: a key that was not active yet, or was retired
/// before that day: "entitlements signed by the old key stay valid until they expire", so the window is
/// NOT tested at `now`); the day has begun at `now`, within the skew (`NotYetIssued`: a staged
/// key cannot sign into its future window); the signature verifies (`BadSignature`); and last, once the
/// signature is authentic, the key is not REVOKED: no window of it has `revoked_from` at or before the
/// day of issue (`Revoked`, an ALARM; one window that says so is enough, whatever the others say). The
/// RETIREMENT of a key (the end of its window) and its REVOCATION are two facts (`KeyWindow::not_after`
/// and `KeyWindow::revoked_from`): the registry fills the second from its revocation rows (S9d, S9i).
///
/// What it cannot see: an entitlement a thief dates BEFORE `revoked_from` verifies like the old honest
/// one (the date is the thief's to write), so the registry sets `revoked_from` to the earliest second
/// the key may have been in other hands, and to the window's `not_before` when that is unknown.
///
/// Implements planning/architecture.md section 2.4 ("The group verifies the payments key") and section
/// 5.11; DRIFT X-DELTA-W1S9E-5 (decision D-W1S9E3): the brief's `verify_entitlement(e, anchors)` has no
/// time, and its check S9E-09 and its mutant (d) presuppose one.
///
/// Caller: S9i's `CredsEntitlementVerifier` (inside the apply of `entitlement_put`, at the leader's
/// stamp), S9d.
pub fn verify_entitlement(
    e: &SignedEntitlement,
    anchors: &AnchorSet,
    now: LeaderTime,
) -> Result<VerifiedEntitlement, EntitlementError> {
    verify_entitlement_with(&Ed25519Verifier, e, anchors, now)
}

/// `verify_entitlement` over a signature verifier of the caller's (see `verify_credential_with`).
///
/// Seam: tests; production uses [`verify_entitlement`].
pub fn verify_entitlement_with(
    verifier: &dyn SignatureVerifier,
    e: &SignedEntitlement,
    anchors: &AnchorSet,
    now: LeaderTime,
) -> Result<VerifiedEntitlement, EntitlementError> {
    // The verified claims are the signed bytes' own, never the copy the value carries.
    let claims = EntitlementClaims::decode_payload(&e.payload)?;
    let issued = claims.issued_at.start();
    let mut key_known = false;
    let mut window_covers = false;
    let mut revoked = false;
    for window in &anchors.payments {
        if window.public != e.key {
            continue;
        }
        key_known = true;
        // The window is tested at the day the entitlement was issued, never at the moment of
        // verification (the mutant moves it to `now`).
        let tested = if azlin_seams::mutant!("creds_entitlement_window_at_now") {
            now
        } else {
            issued
        };
        if window.contains(tested) {
            window_covers = true;
        }
        // The revocation is tested at the same day, and a window that says the key is revoked from a
        // second at or before it is enough: any window of the key, fail closed (the mutants test at
        // `now`, and with `>`).
        if let Some(from) = window.revoked_from {
            let dated = if azlin_seams::mutant!("creds_revocation_at_now") {
                now
            } else {
                issued
            };
            let hit = if azlin_seams::mutant!("creds_revocation_boundary_open") {
                dated > from
            } else {
                dated >= from
            };
            if hit {
                revoked = true;
            }
        }
    }
    if !key_known {
        return Err(EntitlementError::UnknownKey);
    }
    if !window_covers {
        return Err(EntitlementError::KeyWindow);
    }
    let begins_later = match now.checked_add(ISSUE_SKEW) {
        Some(limit) => issued > limit,
        None => false,
    };
    if begins_later && !azlin_seams::mutant!("creds_entitlement_future_ok") {
        return Err(EntitlementError::NotYetIssued);
    }
    let preimage = preimage_of::<EntitlementClaims>(&e.payload);
    let verdict = verifier.verify(
        SigAlg::Ed25519,
        e.key.as_bytes(),
        &preimage,
        e.sig.as_bytes(),
    );
    if verdict.is_err() {
        return Err(EntitlementError::BadSignature);
    }
    // After the signature: an alarm is a fact about an authentic signature, never about a forgery that
    // merely names a revoked key's public key.
    if revoked && !azlin_seams::mutant!("creds_revoked_from_ignored") {
        return Err(EntitlementError::Revoked);
    }
    Ok(VerifiedEntitlement::new(claims, e.key))
}

#[cfg(test)]
mod tests {
    use azlin_keys::Secret;
    use azlin_regbase::KeyWindow;

    use super::*;
    use crate::signer::SeedSigner;

    const DAY: u64 = 86_400;

    fn payments() -> SeedSigner {
        SeedSigner::from_seed(&Secret::from_bytes([41; 32]))
    }

    fn day(n: u64) -> DayStart {
        DayStart::from_start(LeaderTime::from_secs(n * DAY)).expect("a day start")
    }

    fn claims(issued_day: u64) -> EntitlementClaims {
        EntitlementClaims {
            account: AccountId::from_bytes([0x11; 16]),
            kind: AccountKind::Team,
            seats: 25,
            quota: ByteCount::gib(500),
            retention: Span::days(90),
            features: Features::new(vec![SmallText::new("team").expect("a word")])
                .expect("features"),
            period_until: None,
            state: EntitlementState::Active,
            regions_allowed: RegionSet::new(vec![RegionId::new(1)]).expect("regions"),
            issued_at: day(issued_day),
            version: 7,
        }
    }

    /// The payments key: its window ran from day 0 and was RETIRED at day 200 (a newer key replaced
    /// it); the registry may have REVOKED it from a day.
    fn anchors(revoked_from_day: Option<u64>) -> AnchorSet {
        AnchorSet {
            payments: vec![KeyWindow {
                public: payments().public(),
                not_before: LeaderTime::from_secs(0),
                not_after: Some(LeaderTime::from_secs(200 * DAY)),
                revoked_from: revoked_from_day.map(|d| LeaderTime::from_secs(d * DAY)),
            }],
            operator: Vec::new(),
            identity: Vec::new(),
            map_epoch: 5,
        }
    }

    /// The attack of drift row X-DELTA-W1S9E-5: the payments key was retired at day 200 and is stolen
    /// afterwards; the thief signs an entitlement and writes a day inside the old window on it, so that
    /// the retirement rule (the window is tested at the day of issue) accepts it. The registry has
    /// revoked the key from day 150: an entitlement dated at or after that day is refused, as an alarm,
    /// however long after the retirement it is put.
    #[test]
    fn a_stolen_payments_key_cannot_back_date_an_entitlement() {
        let thief_now = LeaderTime::from_secs(300 * DAY + 10);
        let backdated = sign_entitlement(&payments(), claims(160)).expect("signs");
        // the hole: a key nobody marked revoked still verifies the back-dated day
        assert!(
            verify_entitlement(&backdated, &anchors(None), thief_now).is_ok(),
            "a retired key that is not revoked keeps its entitlements"
        );
        // the fix: revoked from day 150, the same entitlement is refused
        assert_eq!(
            verify_entitlement(&backdated, &anchors(Some(150)), thief_now).err(),
            Some(EntitlementError::Revoked)
        );
        // the revocation day itself is refused (the first second belongs to the revoked side)
        let on_the_day = sign_entitlement(&payments(), claims(150)).expect("signs");
        assert_eq!(
            verify_entitlement(&on_the_day, &anchors(Some(150)), thief_now).err(),
            Some(EntitlementError::Revoked)
        );
        // an entitlement the key signed before the revocation stays valid, also long after it: the day of
        // issue decides, not the moment of the check
        let honest = sign_entitlement(&payments(), claims(149)).expect("signs");
        assert!(verify_entitlement(&honest, &anchors(Some(150)), thief_now).is_ok());
        assert!(EntitlementError::Revoked.is_alarm());
    }
}
