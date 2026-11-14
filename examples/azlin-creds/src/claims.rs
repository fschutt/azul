//! The claims of a credential and of a member assertion (planning/architecture.md section 5.11 "Access:
//! entitlements, credentials, assertions, attestations").
//!
//! - **Credential v2** (`azlin-credential-v2`): `account`, `member`, `device` (who), `policy_epoch` (a
//!   policy change invalidates older credentials), `scopes` (only narrows), `issued_at`, `expires_at`,
//!   `key_epoch`, `nonce`, `sso` (identity provider and auth time); plus `family` (DRIFT D-1c, X-D-06:
//!   the family whose token the refresh spent, so a revoked family cuts exactly its credentials; `None`
//!   for an app principal and a link).
//! - **Member assertion** (`azlin-member-assert-v1`): account, member, device, role, groups, issued,
//!   expiry (minutes). "This is how services learn membership without storage access."
//!
//! Each claim set is a CBOR map with unsigned-integer keys (the `K_*` constants below), written with
//! `azlin-cbor`: an optional field is ABSENT when `None` (a `null` is refused), a set is in its one
//! canonical form, a number that does not fit its type is refused. A reader ignores a key it does not
//! know and refuses one `crit` lists (section 5.1 principle 6); because the signature covers the
//! received bytes, an issuer of a newer version may add a field without breaking an older verifier.
//!
//! `Debug` of every claim set prints each id as four bytes and `..`: a log line never carries a whole
//! id (QUALITY rule 26).

use core::fmt;

use azlin_cbor::{decode, IntMap, IntMapBuilder, Value};
use azlin_regbase::{
    AccountId, AccountRole, CredentialNonce, DeviceId, FamilyId, KeyEpoch, LeaderTime, MemberId,
    PolicyEpoch, SmallText, UserGroupId,
};

use crate::canon::{
    build, bytes_of, id16_of, read_time, read_u32, text_of, uint_of, write, Canonical, Short,
    CREDENTIAL_V2, MEMBER_ASSERT_V1, PAYLOAD_LIMITS,
};
use crate::error::{CanonicalError, CredError, WireError};
use crate::scopes::Scopes;
use crate::verify::{check_lifetime, VerifyRules};

/// The most user groups an assertion lists (planning/architecture.md section 1.4 rule 4): a member is in
/// a handful, a large business in a few dozen. est.
pub const MAX_GROUPS: usize = 256;

// The wire keys of a credential's claims.
const K_ACCOUNT: u64 = 1;
const K_MEMBER: u64 = 2;
const K_DEVICE: u64 = 3;
const K_FAMILY: u64 = 4;
const K_POLICY_EPOCH: u64 = 5;
const K_SCOPES: u64 = 6;
const K_ISSUED: u64 = 7;
const K_EXPIRES: u64 = 8;
const K_KEY_EPOCH: u64 = 9;
const K_NONCE: u64 = 10;
const K_SSO: u64 = 11;

// The wire keys of an SSO claim.
const K_SSO_PROVIDER: u64 = 1;
const K_SSO_AUTH: u64 = 2;

// The wire keys of an assertion's claims.
const A_ACCOUNT: u64 = 1;
const A_MEMBER: u64 = 2;
const A_DEVICE: u64 = 3;
const A_ROLE: u64 = 4;
const A_GROUPS: u64 = 5;
const A_ISSUED: u64 = 6;
const A_EXPIRES: u64 = 7;

/// The single sign-on fact of a credential: which identity provider vouched, and when the person
/// signed in there (planning/architecture.md section 5.11: "`sso` | identity provider and auth time").
///
/// DRIFT X-DELTA-W1S9E-6: the plan gives the field no shape; provisional: the provider is a printable
/// ASCII name of 1 to 64 bytes (a `SmallText`), the auth time a leader stamp.
///
/// Caller: S9b (`GrantFacts::sso` becomes this claim in S9i's `claims_of`), S9g (the policy's
/// `sso_max_age` against `auth_time`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SsoClaim {
    /// The identity provider that vouched (1 to 64 printable ASCII bytes).
    pub provider: SmallText,
    /// When the person signed in at the provider (the attestation's auth time).
    pub auth_time: LeaderTime,
}

impl SsoClaim {
    fn to_value(&self) -> Result<Value, CanonicalError> {
        build(
            IntMapBuilder::new()
                .put(K_SSO_PROVIDER, text_of(self.provider.as_str()))
                .put(K_SSO_AUTH, uint_of(self.auth_time.as_secs())),
        )
    }

    fn from_value(value: &Value) -> Result<SsoClaim, WireError> {
        let mut map = IntMap::new(value)?;
        let provider =
            SmallText::new(map.req_text(K_SSO_PROVIDER)?).map_err(|_| WireError::Malformed)?;
        if provider.is_empty() {
            return Err(WireError::Malformed);
        }
        let auth_time = read_time(&mut map, K_SSO_AUTH)?;
        map.finish()?;
        Ok(SsoClaim {
            provider,
            auth_time,
        })
    }
}

/// The claims of a credential v2.
///
/// A device credential names its device and its family and has no scopes; the credential of an app
/// principal, a link or a request has no device and no family and always carries scopes (a credential
/// of no device without scopes would be unrestricted by anything: it is `Malformed`).
///
/// Implements planning/architecture.md section 5.11 ("Credential v2") and DRIFT D-1c.
///
/// Caller: S9i (`claims_of` builds the grant's claims, the issuer stamps the key epoch), S9g (reads the
/// verified claims), the tests.
#[derive(Clone, PartialEq, Eq)]
pub struct CredentialClaims {
    /// The account.
    pub account: AccountId,
    /// The member.
    pub member: MemberId,
    /// The device; `None` for an app principal, a link or a request.
    pub device: Option<DeviceId>,
    /// The family whose token the refresh spent; `None` for an app principal and a link (D-1c).
    pub family: Option<FamilyId>,
    /// The account's policy epoch when it was issued: a newer epoch invalidates it.
    pub policy_epoch: PolicyEpoch,
    /// What the credential may do at most (only narrows); `None` for a device credential.
    pub scopes: Option<Scopes>,
    /// When the credential was issued (the leader's stamp).
    pub issued_at: LeaderTime,
    /// The first second it is no longer valid (the leader's stamp).
    pub expires_at: LeaderTime,
    /// The epoch of the credential key that signed it; the certificate's epoch must equal it.
    pub key_epoch: KeyEpoch,
    /// The nonce a revocation of this one credential names.
    pub nonce: CredentialNonce,
    /// The single sign-on fact, when the account requires one.
    pub sso: Option<SsoClaim>,
}

impl fmt::Debug for CredentialClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("CredentialClaims")
            .field("account", &Short(self.account.as_bytes()))
            .field("member", &Short(self.member.as_bytes()))
            .field(
                "device",
                &self.device.map(|d| format!("{:?}", Short(d.as_bytes()))),
            )
            .field("family", &self.family)
            .field("policy_epoch", &self.policy_epoch)
            .field("scopes", &self.scopes)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("key_epoch", &self.key_epoch)
            .field("nonce", &self.nonce)
            .field("sso", &self.sso)
            .finish()
    }
}

impl Canonical for CredentialClaims {
    const DOMAIN: azlin_regbase::Domain = CREDENTIAL_V2;

    fn canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        write(&self.to_value()?, &PAYLOAD_LIMITS)
    }
}

impl CredentialClaims {
    fn to_value(&self) -> Result<Value, CanonicalError> {
        let mut b = IntMapBuilder::new()
            .put(K_ACCOUNT, bytes_of(self.account.as_bytes()))
            .put(K_MEMBER, bytes_of(self.member.as_bytes()));
        if let Some(device) = &self.device {
            b = b.put(K_DEVICE, bytes_of(device.as_bytes()));
        }
        if let Some(family) = &self.family {
            b = b.put(K_FAMILY, bytes_of(family.as_bytes()));
        }
        b = b.put(K_POLICY_EPOCH, uint_of(u64::from(self.policy_epoch.get())));
        if let Some(scopes) = &self.scopes {
            b = b.put(K_SCOPES, scopes.to_value());
        }
        b = b
            .put(K_ISSUED, uint_of(self.issued_at.as_secs()))
            .put(K_EXPIRES, uint_of(self.expires_at.as_secs()))
            .put(K_KEY_EPOCH, uint_of(u64::from(self.key_epoch.get())));
        if !azlin_seams::mutant!("creds_nonce_unsigned") {
            b = b.put(K_NONCE, bytes_of(self.nonce.as_bytes()));
        }
        if let Some(sso) = &self.sso {
            b = b.put(K_SSO, sso.to_value()?);
        }
        build(b)
    }

    /// The claims of the signed bytes (the payload of a credential), STRICTLY: a missing key, a wrong
    /// type, a set that is not canonical, a scope of a kind this build does not know, a `crit` it does
    /// not understand are all refused; a key nobody asked for and `crit` does not list is ignored.
    ///
    /// Caller: `Credential::decode`, `verify_credential` (the verified claims are read from the signed
    /// bytes themselves, never from a copy).
    pub fn decode_payload(bytes: &[u8]) -> Result<CredentialClaims, WireError> {
        let value = decode(bytes, &PAYLOAD_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        let account = AccountId::from_bytes(map.req_bytes_n::<16>(K_ACCOUNT)?);
        let member = MemberId::from_bytes(map.req_bytes_n::<16>(K_MEMBER)?);
        let device = map.opt_bytes_n::<16>(K_DEVICE)?.map(DeviceId::from_bytes);
        let family = map.opt_bytes_n::<16>(K_FAMILY)?.map(FamilyId::from_bytes);
        let policy_epoch = PolicyEpoch::new(read_u32(&mut map, K_POLICY_EPOCH)?);
        let scopes = match map.opt_array(K_SCOPES)? {
            Some(items) => Some(Scopes::from_values(items)?),
            None => None,
        };
        let issued_at = read_time(&mut map, K_ISSUED)?;
        let expires_at = read_time(&mut map, K_EXPIRES)?;
        let key_epoch = KeyEpoch::new(read_u32(&mut map, K_KEY_EPOCH)?);
        let nonce = CredentialNonce::from_bytes(map.req_bytes_n::<16>(K_NONCE)?);
        let sso = match map.opt_value(K_SSO)? {
            Some(value) => Some(SsoClaim::from_value(value)?),
            None => None,
        };
        map.finish()?;
        Ok(CredentialClaims {
            account,
            member,
            device,
            family,
            policy_epoch,
            scopes,
            issued_at,
            expires_at,
            key_epoch,
            nonce,
            sso,
        })
    }

    /// The rules of the SHAPE of a credential, checked by the verifier and, before signing, by the
    /// issuer: `expires_at` after `issued_at` (`Malformed`), at most `max_credential_lifetime` long
    /// (`LifetimeTooLong`), a credential of no device carries scopes and a device credential names its
    /// family (`Malformed`), scopes that are present are not empty (`Malformed`).
    ///
    /// Caller: `verify_credential`, `CredentialIssuer::issue`.
    pub(crate) fn check_shape(&self, rules: &VerifyRules) -> Result<(), CredError> {
        check_lifetime(
            self.issued_at,
            self.expires_at,
            rules.max_credential_lifetime,
        )?;
        if self.device.is_none() && self.scopes.is_none() {
            return Err(CredError::Malformed);
        }
        if self.device.is_some() && self.family.is_none() {
            return Err(CredError::Malformed);
        }
        if self.scopes.as_ref().is_some_and(Scopes::is_empty) {
            return Err(CredError::Malformed);
        }
        Ok(())
    }
}

/// What the registry's refresh decided, as a credential asks for it: the claims WITHOUT the key epoch.
/// The issuer stamps the epoch from its certificate, so a caller cannot pass another (planning/
/// architecture.md section 2.4; the brief's check S9E-10).
///
/// Implements planning/architecture.md section 5.11 (the fields of `GrantFacts` in S9b are these).
///
/// Caller: S9i (`finish_refresh` turns a `Grant` into one), `CredentialIssuer::issue`.
#[derive(Clone, PartialEq, Eq)]
pub struct GrantClaims {
    /// The account.
    pub account: AccountId,
    /// The member.
    pub member: MemberId,
    /// The device; `None` for an app principal, a link or a request.
    pub device: Option<DeviceId>,
    /// The family whose token the refresh spent; `None` for an app principal and a link.
    pub family: Option<FamilyId>,
    /// The account's policy epoch.
    pub policy_epoch: PolicyEpoch,
    /// What the credential may do at most; `None` for a device credential.
    pub scopes: Option<Scopes>,
    /// When the credential is issued (the leader's stamp).
    pub issued_at: LeaderTime,
    /// The first second it is no longer valid.
    pub expires_at: LeaderTime,
    /// The nonce a revocation of this credential names.
    pub nonce: CredentialNonce,
    /// The single sign-on fact, when the account requires one.
    pub sso: Option<SsoClaim>,
}

impl GrantClaims {
    /// The claims of a credential signed by the key of epoch `key_epoch`.
    pub(crate) fn with_key_epoch(self, key_epoch: KeyEpoch) -> CredentialClaims {
        CredentialClaims {
            account: self.account,
            member: self.member,
            device: self.device,
            family: self.family,
            policy_epoch: self.policy_epoch,
            scopes: self.scopes,
            issued_at: self.issued_at,
            expires_at: self.expires_at,
            key_epoch,
            nonce: self.nonce,
            sso: self.sso,
        }
    }
}

impl fmt::Debug for GrantClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("GrantClaims")
            .field("account", &Short(self.account.as_bytes()))
            .field("member", &Short(self.member.as_bytes()))
            .field(
                "device",
                &self.device.map(|d| format!("{:?}", Short(d.as_bytes()))),
            )
            .field("family", &self.family)
            .field("policy_epoch", &self.policy_epoch)
            .field("scopes", &self.scopes)
            .field("issued_at", &self.issued_at)
            .field("expires_at", &self.expires_at)
            .field("nonce", &self.nonce)
            .field("sso", &self.sso)
            .finish()
    }
}

/// The claims of a member assertion: who the metadata group vouches for, for minutes.
///
/// The `groups` list is SORTED and without duplicates, at most [`MAX_GROUPS`]: the issuer makes it so
/// (`AssertionIssuer::issue`), the encoder refuses any other list, and a verifier refuses any other
/// encoding, so two encodings of one set never both verify.
///
/// Implements planning/architecture.md section 5.11 ("Member assertion": account, member, device,
/// role, groups, issued, expiry).
///
/// Caller: S9g (`assertion_claims` builds it from the registry), `AssertionIssuer::issue`, the
/// services A2 to A5 (read the verified claims).
#[derive(Clone, PartialEq, Eq)]
pub struct AssertionClaims {
    /// The account.
    pub account: AccountId,
    /// The member.
    pub member: MemberId,
    /// The device the assertion was fetched for.
    pub device: DeviceId,
    /// The member's role in the account.
    pub role: AccountRole,
    /// The user groups the member is in: sorted, without duplicates, at most [`MAX_GROUPS`].
    pub groups: Vec<UserGroupId>,
    /// When the assertion was issued (the leader's stamp).
    pub issued: LeaderTime,
    /// The first second it is no longer valid; at most ten minutes after `issued`.
    pub expires: LeaderTime,
}

impl fmt::Debug for AssertionClaims {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AssertionClaims")
            .field("account", &Short(self.account.as_bytes()))
            .field("member", &Short(self.member.as_bytes()))
            .field("device", &Short(self.device.as_bytes()))
            .field("role", &self.role)
            .field("groups", &self.groups.len())
            .field("issued", &self.issued)
            .field("expires", &self.expires)
            .finish()
    }
}

/// Whether a list of groups is in its one canonical form: strictly ascending, at most [`MAX_GROUPS`].
pub(crate) fn groups_are_canonical(groups: &[UserGroupId]) -> bool {
    groups.len() <= MAX_GROUPS
        && groups
            .windows(2)
            .all(|pair| matches!(pair, [a, b] if a < b))
}

impl Canonical for AssertionClaims {
    const DOMAIN: azlin_regbase::Domain = MEMBER_ASSERT_V1;

    fn canonical_bytes(&self) -> Result<Vec<u8>, CanonicalError> {
        if !groups_are_canonical(&self.groups) {
            return Err(CanonicalError::NotCanonical);
        }
        let groups = self
            .groups
            .iter()
            .map(|group| bytes_of(group.as_bytes()))
            .collect();
        let value = build(
            IntMapBuilder::new()
                .put(A_ACCOUNT, bytes_of(self.account.as_bytes()))
                .put(A_MEMBER, bytes_of(self.member.as_bytes()))
                .put(A_DEVICE, bytes_of(self.device.as_bytes()))
                .put(A_ROLE, text_of(self.role.as_str()))
                .put(A_GROUPS, Value::Array(groups))
                .put(A_ISSUED, uint_of(self.issued.as_secs()))
                .put(A_EXPIRES, uint_of(self.expires.as_secs())),
        )?;
        write(&value, &PAYLOAD_LIMITS)
    }
}

impl AssertionClaims {
    /// The claims of the signed bytes (the payload of an assertion), STRICTLY: the groups must be
    /// strictly ascending and at most [`MAX_GROUPS`] (`NotCanonical`, `TooMany`), the role a word of the
    /// vocabulary (`UnknownWord`); unknown keys are ignored unless `crit` lists them.
    ///
    /// Caller: `Assertion::decode`, `verify_assertion`.
    pub fn decode_payload(bytes: &[u8]) -> Result<AssertionClaims, WireError> {
        let value = decode(bytes, &PAYLOAD_LIMITS)?;
        let mut map = IntMap::new(&value)?;
        let account = AccountId::from_bytes(map.req_bytes_n::<16>(A_ACCOUNT)?);
        let member = MemberId::from_bytes(map.req_bytes_n::<16>(A_MEMBER)?);
        let device = DeviceId::from_bytes(map.req_bytes_n::<16>(A_DEVICE)?);
        let role = map
            .req_text(A_ROLE)?
            .parse::<AccountRole>()
            .map_err(|_| WireError::UnknownWord)?;
        let items = map.req_array(A_GROUPS)?;
        if items.len() > MAX_GROUPS {
            return Err(WireError::TooMany);
        }
        let mut groups: Vec<UserGroupId> = Vec::with_capacity(items.len());
        for item in items {
            groups.push(UserGroupId::from_bytes(id16_of(item)?));
        }
        if !groups_are_canonical(&groups) {
            return Err(WireError::NotCanonical);
        }
        let issued = read_time(&mut map, A_ISSUED)?;
        let expires = read_time(&mut map, A_EXPIRES)?;
        map.finish()?;
        Ok(AssertionClaims {
            account,
            member,
            device,
            role,
            groups,
            issued,
            expires,
        })
    }
}
