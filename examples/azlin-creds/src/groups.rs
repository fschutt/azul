//! Which metadata group an account lives on (planning/architecture.md section 2.4: "Accounts map to a
//! metadata group through a field in the map"; decision D-W1S9E3, drift row X-DELTA-W1S9E-3).
//!
//! THE HOLE THIS CLOSES. Every metadata group has an identity key of its own and certifies the credential
//! keys of its own voters, and the anchors name every group's identity key. A credential or an assertion
//! that verifies under the anchors therefore proves that SOME group of the map vouches for it, not that
//! the group that OWNS the account does: a voter of group 2 holds a perfectly valid credential key and
//! could vouch for an account that lives on group 1. Nothing that is signed says which group an account is
//! placed on: the placement is the map's.
//!
//! THE SEAM. [`AccountGroups`] is how a verifier says where an account lives. `verify_credential`,
//! `verify_assertion` and their variants TAKE it as a required parameter, so that no caller can forget
//! the check: once the chain verified (two signatures), the group of the verified certificate must be the
//! group `group_of` names for the account, or the call refuses with `GroupMismatch`, which is an ALARM:
//! the chain is authentic and the voter has no right to vouch for the account. An account the verifier
//! does not know is `UnknownAccount`, a refusal and never a default group.
//!
//! There is NO implementation here that answers for every account, and none may be added outside
//! `#[cfg(test)]` and test support: [`InstalledAccount`] knows exactly one account. The answer must come
//! from the verifier's own state (the registry's placement, a service's install config), never from the
//! credential that is being verified.

use core::fmt;

use azlin_regbase::{AccountId, MetaGroupId};

use crate::canon::Short;
use crate::error::Placement;

/// The metadata group each account lives on, as the verifier knows it.
///
/// `None` means "this verifier does not know the account": the verification refuses
/// (`UnknownAccount`), it never falls back to a group. The answer is a fact of the verifier's OWN state,
/// the registry's placement or the install config of a service; an implementation that returns the group
/// of the credential being verified, or any group for an account it has not placed, switches the check
/// off. During an account's move to another group (planning/architecture.md section 2.4: "an account
/// moves to another group whole") the credentials of its old group meet `GroupMismatch` until the
/// registry's answer changes: when the answer changes is the registry's decision.
///
/// Implements planning/architecture.md section 2.4 ("Accounts map to a metadata group through a field in
/// the map") and decision D-W1S9E3 (drift row X-DELTA-W1S9E-3).
///
/// Seam: S9g's `check_connection` and the nodes' verb handlers (the registry's placement of accounts,
/// from the map), S9i; the services A2 to A5 through [`InstalledAccount`]; the tests through their own
/// fixed tables.
pub trait AccountGroups {
    /// The metadata group `account` lives on, or `None` when this verifier does not know the account.
    fn group_of(&self, account: &AccountId) -> Option<MetaGroupId>;
}

/// The one account a service was installed for, and the metadata group it lives on: the answer of a
/// service that knows its account from its install config (planning/architecture.md section 5.11:
/// services verify an assertion against "the account id set at install"; the group is set beside it).
///
/// It answers for exactly that account and for no other (`None`), so it is not a permissive
/// implementation: a credential of any other account is `UnknownAccount`. The group must be the one the
/// map places the account on; taking it from the assertion that is being verified would switch the check
/// off.
///
/// Implements planning/architecture.md sections 2.4 and 5.11 and decision D-W1S9E3 (drift row
/// X-DELTA-W1S9E-3).
///
/// Caller: the services A2 to A5 (meet in org mode, the mail sending relay, the identity service's
/// admin API) pass one to `verify_assertion`, built from their install config; the tests.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct InstalledAccount {
    account: AccountId,
    group: MetaGroupId,
}

/// Prints the account as four bytes and `..`: never the whole id.
impl fmt::Debug for InstalledAccount {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("InstalledAccount")
            .field("account", &Short(self.account.as_bytes()))
            .field("group", &self.group)
            .finish()
    }
}

impl InstalledAccount {
    /// The installation of `account`, which lives on metadata group `group`.
    pub const fn new(account: AccountId, group: MetaGroupId) -> InstalledAccount {
        InstalledAccount { account, group }
    }

    /// The account this service was installed for.
    pub const fn account(&self) -> AccountId {
        self.account
    }

    /// The metadata group the account lives on.
    pub const fn group(&self) -> MetaGroupId {
        self.group
    }
}

impl AccountGroups for InstalledAccount {
    fn group_of(&self, account: &AccountId) -> Option<MetaGroupId> {
        if *account == self.account {
            Some(self.group)
        } else {
            None
        }
    }
}

/// The last step of `verify_credential` and `verify_assertion`, after the chain and both signatures
/// verified: the account is known (`UnknownAccount`) and the group whose identity key certified the
/// credential key is the group the account lives on (`GroupMismatch`).
pub(crate) fn check_placement(
    groups: &dyn AccountGroups,
    account: &AccountId,
    certified: MetaGroupId,
) -> Result<(), Placement> {
    let Some(home) = groups.group_of(account) else {
        return Err(Placement::UnknownAccount);
    };
    if home != certified && !azlin_seams::mutant!("creds_group_comparison_dropped") {
        return Err(Placement::GroupMismatch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use azlin_keys::Secret;
    use azlin_regbase::{
        AccountRole, AnchorSet, CredentialNonce, DeviceId, FamilyId, KeyEpoch, KeyWindow,
        LeaderTime, MemberId, PolicyEpoch,
    };

    use super::*;
    use crate::assertion::{verify_assertion, verify_assertion_bytes, Assertion};
    use crate::cert::certify;
    use crate::claims::{AssertionClaims, GrantClaims};
    use crate::credential::{verify_credential, verify_credential_bytes, Credential};
    use crate::error::{AssertError, CredError};
    use crate::issuer::{AssertionIssuer, CredentialIssuer};
    use crate::signer::{SeedSigner, Signer};
    use crate::verify::{NodeTime, VerifierNow, VerifyRules};

    const ISSUED: u64 = 100_000;
    const GROUP_ONE: MetaGroupId = MetaGroupId::new(1);
    const GROUP_TWO: MetaGroupId = MetaGroupId::new(2);

    fn account() -> AccountId {
        AccountId::from_bytes([0x11; 16])
    }

    fn signer(seed: u8) -> SeedSigner {
        SeedSigner::from_seed(&Secret::from_bytes([seed; 32]))
    }

    fn at(secs: u64) -> LeaderTime {
        LeaderTime::from_secs(secs)
    }

    fn now() -> VerifierNow {
        VerifierNow::trusted(NodeTime::from_secs(ISSUED + 10))
    }

    /// Two metadata groups of one map: the anchors name the identity key of each, so a chain of either
    /// verifies; each group has a credential key of its own, certified by its own identity key.
    struct TwoGroups {
        anchors: AnchorSet,
        cred_one: SeedSigner,
        cred_two: SeedSigner,
        cert_one: crate::cert::Certificate,
        cert_two: crate::cert::Certificate,
    }

    impl TwoGroups {
        fn new() -> TwoGroups {
            let (identity_one, identity_two) = (signer(1), signer(3));
            let (cred_one, cred_two) = (signer(2), signer(4));
            let window = |s: &SeedSigner| KeyWindow {
                public: s.public(),
                not_before: at(0),
                not_after: None,
                revoked_from: None,
            };
            let anchors = AnchorSet {
                payments: Vec::new(),
                operator: Vec::new(),
                identity: vec![
                    (GROUP_ONE, window(&identity_one)),
                    (GROUP_TWO, window(&identity_two)),
                ],
                map_epoch: 1,
            };
            let certify_for = |identity: &SeedSigner, group, cred: &SeedSigner| {
                certify(
                    identity,
                    group,
                    KeyEpoch::new(7),
                    cred.public(),
                    at(50_000),
                    at(50_000 + 86_400),
                )
                .expect("a certificate")
            };
            let cert_one = certify_for(&identity_one, GROUP_ONE, &cred_one);
            let cert_two = certify_for(&identity_two, GROUP_TWO, &cred_two);
            TwoGroups {
                anchors,
                cred_one,
                cred_two,
                cert_one,
                cert_two,
            }
        }

        /// The credential of the account, issued by a voter of group 1 or of group 2.
        fn credential(&self, group: u8) -> Credential {
            let (cred, cert) = if group == 1 {
                (&self.cred_one, self.cert_one)
            } else {
                (&self.cred_two, self.cert_two)
            };
            let issuer =
                CredentialIssuer::new(cred, cert, &self.anchors, at(ISSUED)).expect("an issuer");
            issuer
                .issue(GrantClaims {
                    account: account(),
                    member: MemberId::from_bytes([0x22; 16]),
                    device: Some(DeviceId::from_bytes([0x33; 16])),
                    family: Some(FamilyId::from_bytes([0x44; 16])),
                    policy_epoch: PolicyEpoch::new(3),
                    scopes: None,
                    issued_at: at(ISSUED),
                    expires_at: at(ISSUED + 3_600),
                    nonce: CredentialNonce::from_bytes([0x55; 16]),
                    sso: None,
                })
                .expect("a credential")
        }

        /// The member assertion of the account, issued by a voter of group 1 or of group 2.
        fn assertion(&self, group: u8) -> Assertion {
            let (cred, cert) = if group == 1 {
                (&self.cred_one, self.cert_one)
            } else {
                (&self.cred_two, self.cert_two)
            };
            let issuer =
                AssertionIssuer::new(cred, cert, &self.anchors, at(ISSUED)).expect("an issuer");
            issuer
                .issue(AssertionClaims {
                    account: account(),
                    member: MemberId::from_bytes([0x22; 16]),
                    device: DeviceId::from_bytes([0x33; 16]),
                    role: AccountRole::Member,
                    groups: Vec::new(),
                    issued: at(ISSUED),
                    expires: at(ISSUED + 300),
                })
                .expect("an assertion")
        }
    }

    /// The attack of drift row X-DELTA-W1S9E-3 on a credential: the account lives on group 1; a voter
    /// of group 2 holds a credential key certified by group 2's identity key, which the map names, so the
    /// chain is authentic, and it vouches for the account. The placement refuses it, as an alarm.
    #[test]
    fn a_voter_of_another_group_cannot_vouch_for_the_account() {
        let map = TwoGroups::new();
        let rules = VerifyRules::default();
        let placed = InstalledAccount::new(account(), GROUP_ONE);
        // the control: the group that owns the account vouches for it
        let own = map.credential(1);
        let verified = verify_credential(&own, &map.anchors, &placed, now(), &rules)
            .expect("the owning group's voter may vouch");
        assert_eq!(verified.group(), GROUP_ONE);
        assert_eq!(verified.account(), account());
        // the attack: the same account, a chain that is authentic under the anchors, another group
        let forged = map.credential(2);
        assert_eq!(
            verify_credential(&forged, &map.anchors, &placed, now(), &rules).err(),
            Some(CredError::GroupMismatch)
        );
        // the bytes of a request meet the same refusal
        let bytes = forged.encode().expect("the bytes");
        assert_eq!(
            verify_credential_bytes(&bytes, &map.anchors, &placed, now(), &rules).err(),
            Some(CredError::GroupMismatch)
        );
        // an account the verifier has not placed is a refusal too, never a default group
        let elsewhere = InstalledAccount::new(AccountId::from_bytes([0x99; 16]), GROUP_ONE);
        assert_eq!(
            verify_credential(&own, &map.anchors, &elsewhere, now(), &rules).err(),
            Some(CredError::UnknownAccount)
        );
        assert!(CredError::GroupMismatch.is_alarm());
        assert!(!CredError::UnknownAccount.is_alarm());
    }

    /// The same attack on a member assertion: a service installed for the account, which lives on
    /// group 1, is shown an assertion of a voter of group 2.
    #[test]
    fn a_voter_of_another_group_cannot_vouch_for_the_account_in_an_assertion() {
        let map = TwoGroups::new();
        let rules = VerifyRules::default();
        let installed = InstalledAccount::new(account(), GROUP_ONE);
        let own = map.assertion(1);
        let verified = verify_assertion(&own, &map.anchors, &installed, now(), &rules, &account())
            .expect("the owning group's voter may vouch");
        assert_eq!(verified.group(), GROUP_ONE);
        let forged = map.assertion(2);
        assert_eq!(
            verify_assertion(&forged, &map.anchors, &installed, now(), &rules, &account()).err(),
            Some(AssertError::GroupMismatch)
        );
        let bytes = forged.encode().expect("the bytes");
        assert_eq!(
            verify_assertion_bytes(&bytes, &map.anchors, &installed, now(), &rules, &account())
                .err(),
            Some(AssertError::GroupMismatch)
        );
        assert!(AssertError::GroupMismatch.is_alarm());
    }

    /// `InstalledAccount` knows one account and answers `None` for every other: it is not permissive.
    #[test]
    fn an_installed_account_knows_exactly_one_account() {
        let installed = InstalledAccount::new(account(), GROUP_TWO);
        assert_eq!(installed.group_of(&account()), Some(GROUP_TWO));
        for other in [[0x00u8; 16], [0x12; 16], [0xFF; 16]] {
            assert_eq!(installed.group_of(&AccountId::from_bytes(other)), None);
        }
        assert_eq!(installed.account(), account());
        assert_eq!(installed.group(), GROUP_TWO);
    }
}
