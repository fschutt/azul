//! D-W1S9E3 / drift row X-DELTA-W1S9E-3 (planning/architecture.md section 2.4: "Accounts map to a
//! metadata group through a field in the map"): a credential or an assertion verifies only when the
//! group whose identity key certified its credential key is the group the VERIFIER says the account lives
//! on (`AccountGroups`, a required parameter), and the check runs after both signatures, so that an
//! alarm (`GroupMismatch`) is a fact about an authentic chain. The attack itself has its named test inside
//! the crate (`a_voter_of_another_group_cannot_vouch_for_the_account`, the killer of the replayable
//! patch); these tests cover every entry point, the order, the seam and the alarm classes.

use std::cell::RefCell;

use azlin_creds::{
    certify, verify_assertion, verify_assertion_bytes, verify_assertion_with, verify_credential,
    verify_credential_bytes, verify_credential_with, AccountGroups, AssertError, Assertion,
    AssertionClaims, AssertionIssuer, Certificate, CredError, EntitlementError, InstalledAccount,
    SeedSigner, Signer,
};
use azlin_regbase::{
    AccountId, AccountRole, AnchorSet, DeviceId, KeyWindow, MemberId, MetaGroupId,
};
use azlin_seams::Ed25519Verifier;

use crate::support::*;

const GROUP_TWO: MetaGroupId = MetaGroupId::new(2);

fn account() -> AccountId {
    grant().account
}

/// A map of two metadata groups: group 1 is the fixture (identity seed 1, credential seed 2); group 2
/// has its own identity key (seed 3) and credential key (seed 4). The anchors name both identity keys,
/// so a chain of either verifies; the placement decides whose voucher counts.
struct Map {
    anchors: AnchorSet,
    one: Fixture,
    cred_two: SeedSigner,
    cert_two: Certificate,
}

fn map() -> Map {
    let one = fixture();
    let identity_two = signer(3);
    let cred_two = signer(4);
    let mut anchors = one.anchors.clone();
    anchors.identity.push((
        GROUP_TWO,
        KeyWindow {
            public: identity_two.public(),
            not_before: at(0),
            not_after: None,
            revoked_from: None,
        },
    ));
    let cert_two = certify(
        &identity_two,
        GROUP_TWO,
        EPOCH,
        cred_two.public(),
        at(CERT_FROM),
        at(CERT_FROM + CERT_SPAN),
    )
    .expect("the certificate of group 2");
    Map {
        anchors,
        one,
        cred_two,
        cert_two,
    }
}

/// A placement that remembers every question it was asked.
struct Recording {
    table: FixedGroups,
    asked: RefCell<Vec<AccountId>>,
}

impl Recording {
    fn new(table: FixedGroups) -> Recording {
        Recording {
            table,
            asked: RefCell::new(Vec::new()),
        }
    }

    fn questions(&self) -> Vec<AccountId> {
        self.asked.borrow().clone()
    }
}

impl AccountGroups for Recording {
    fn group_of(&self, account: &AccountId) -> Option<MetaGroupId> {
        self.asked.borrow_mut().push(*account);
        self.table.group_of(account)
    }
}

fn assertion_claims() -> AssertionClaims {
    AssertionClaims {
        account: account(),
        member: MemberId::from_bytes([0x22; 16]),
        device: DeviceId::from_bytes([0x33; 16]),
        role: AccountRole::Member,
        groups: Vec::new(),
        issued: at(ISSUED),
        expires: at(ISSUED + 300),
    }
}

/// The assertion of the account issued through the real issuer of a voter of group 1 or of group 2.
fn assertion_by(m: &Map, group_two: bool) -> Assertion {
    if group_two {
        AssertionIssuer::new(&m.cred_two, m.cert_two, &m.anchors, at(ISSUED))
            .expect("the probe verifies")
            .issue(assertion_claims())
            .expect("an assertion")
    } else {
        AssertionIssuer::new(&m.one.cred, m.one.cert, &m.anchors, at(ISSUED))
            .expect("the probe verifies")
            .issue(assertion_claims())
            .expect("an assertion")
    }
}

#[test]
fn s9e3_the_group_that_owns_the_account_vouches_for_it_through_every_entry_point() {
    let m = map();
    let credential = signed_credential(&m.one.cred, m.one.cert, claims_of(grant()));
    let placed = placement();
    let verified = verify_credential(&credential, &m.anchors, &placed, now(ISSUED), &m.one.rules)
        .expect("verifies");
    assert_eq!(verified.group(), GROUP);
    assert_eq!(verified.account(), account());
    let bytes = credential.encode().expect("the bytes");
    assert!(
        verify_credential_bytes(&bytes, &m.anchors, &placed, now(ISSUED), &m.one.rules).is_ok()
    );
    assert!(verify_credential_with(
        &Ed25519Verifier,
        &credential,
        &m.anchors,
        &placed,
        now(ISSUED),
        &m.one.rules
    )
    .is_ok());
}

#[test]
fn s9e3_a_voter_of_another_group_is_group_mismatch_through_every_entry_point() {
    let m = map();
    let forged = signed_credential(&m.cred_two, m.cert_two, claims_of(grant()));
    let placed = placement(); // the account lives on group 1
    let expected = Some(CredError::GroupMismatch);
    assert_eq!(
        verify_credential(&forged, &m.anchors, &placed, now(ISSUED), &m.one.rules).err(),
        expected
    );
    assert_eq!(
        verify_credential_with(
            &Ed25519Verifier,
            &forged,
            &m.anchors,
            &placed,
            now(ISSUED),
            &m.one.rules
        )
        .err(),
        expected
    );
    let bytes = forged.encode().expect("the bytes");
    assert_eq!(
        verify_credential_bytes(&bytes, &m.anchors, &placed, now(ISSUED), &m.one.rules).err(),
        expected
    );
    // The verdict is the placement's: where the account lives on group 2, the same credential
    // verifies and the owner's of group 1 no longer does.
    let on_two = FixedGroups::of(&[(account(), GROUP_TWO)]);
    let verified = verify_credential(&forged, &m.anchors, &on_two, now(ISSUED), &m.one.rules)
        .expect("group 2 owns the account now");
    assert_eq!(verified.group(), GROUP_TWO);
    let own = signed_credential(&m.one.cred, m.one.cert, claims_of(grant()));
    assert_eq!(
        verify_credential(&own, &m.anchors, &on_two, now(ISSUED), &m.one.rules).err(),
        expected
    );
}

#[test]
fn s9e3_an_account_the_verifier_does_not_know_is_refused_and_never_given_a_group() {
    let m = map();
    let credential = signed_credential(&m.one.cred, m.one.cert, claims_of(grant()));
    let strangers = FixedGroups::of(&[(AccountId::from_bytes([0x77; 16]), GROUP)]);
    for groups in [FixedGroups::of(&[]), strangers] {
        assert_eq!(
            verify_credential(&credential, &m.anchors, &groups, now(ISSUED), &m.one.rules).err(),
            Some(CredError::UnknownAccount)
        );
    }
    // the same for an assertion, and for a service installed for another account's row
    let assertion = assertion_by(&m, false);
    let elsewhere = InstalledAccount::new(AccountId::from_bytes([0x77; 16]), GROUP);
    assert_eq!(
        verify_assertion(
            &assertion,
            &m.anchors,
            &elsewhere,
            now(ISSUED),
            &m.one.rules,
            &account()
        )
        .err(),
        Some(AssertError::UnknownAccount)
    );
}

#[test]
fn s9e3_the_placement_is_asked_once_with_the_signed_account_and_only_after_both_signatures() {
    let m = map();
    let credential = signed_credential(&m.one.cred, m.one.cert, claims_of(grant()));
    let recording = Recording::new(placement());
    verify_credential(
        &credential,
        &m.anchors,
        &recording,
        now(ISSUED),
        &m.one.rules,
    )
    .expect("verifies");
    assert_eq!(
        recording.questions(),
        vec![account()],
        "once, with the signed account"
    );

    // Nothing that is refused before the chain is authentic asks the placement a thing: not a
    // credential that expired, not a forged signature, not a certificate of no identity key, and not a
    // forged voucher of the OTHER group (a forgery raises no alarm: it is BadSignature).
    let never = Recording::new(placement());
    let expired = now(ISSUED + LIFETIME);
    assert_eq!(
        verify_credential(&credential, &m.anchors, &never, expired, &m.one.rules).err(),
        Some(CredError::Expired)
    );
    let bad_signature = signed_credential(&signer(8), m.one.cert, claims_of(grant()));
    assert_eq!(
        verify_credential(
            &bad_signature,
            &m.anchors,
            &never,
            now(ISSUED),
            &m.one.rules
        )
        .err(),
        Some(CredError::BadSignature)
    );
    let foreign = signed_cert(
        &signer(9),
        GROUP,
        EPOCH,
        m.one.cred.public(),
        CERT_FROM,
        CERT_FROM + CERT_SPAN,
    );
    let no_identity = signed_credential(&m.one.cred, foreign, claims_of(grant()));
    assert_eq!(
        verify_credential(&no_identity, &m.anchors, &never, now(ISSUED), &m.one.rules).err(),
        Some(CredError::UnknownIdentity)
    );
    let forged_voucher = signed_credential(&signer(8), m.cert_two, claims_of(grant()));
    let error = verify_credential(
        &forged_voucher,
        &m.anchors,
        &never,
        now(ISSUED),
        &m.one.rules,
    )
    .err();
    assert_eq!(error, Some(CredError::BadSignature));
    assert!(
        !error.is_some_and(CredError::is_alarm),
        "a forgery is no alarm"
    );
    assert!(
        never.questions().is_empty(),
        "no refusal before the chain asked the placement"
    );
}

#[test]
fn s9e3_an_assertion_of_another_group_is_group_mismatch_and_the_installed_account_comes_first() {
    let m = map();
    let installed = InstalledAccount::new(account(), GROUP);
    let own = assertion_by(&m, false);
    let verified = verify_assertion(
        &own,
        &m.anchors,
        &installed,
        now(ISSUED),
        &m.one.rules,
        &account(),
    )
    .expect("the owning group's voter may vouch");
    assert_eq!(verified.group(), GROUP);

    let forged = assertion_by(&m, true);
    let expected = Some(AssertError::GroupMismatch);
    assert_eq!(
        verify_assertion(
            &forged,
            &m.anchors,
            &installed,
            now(ISSUED),
            &m.one.rules,
            &account()
        )
        .err(),
        expected
    );
    assert_eq!(
        verify_assertion_with(
            &Ed25519Verifier,
            &forged,
            &m.anchors,
            &installed,
            now(ISSUED),
            &m.one.rules,
            &account()
        )
        .err(),
        expected
    );
    let bytes = forged.encode().expect("the bytes");
    assert_eq!(
        verify_assertion_bytes(
            &bytes,
            &m.anchors,
            &installed,
            now(ISSUED),
            &m.one.rules,
            &account()
        )
        .err(),
        expected
    );

    // A service installed for another account refuses on the account, before any signature and before
    // any question to the placement.
    let recording = Recording::new(placement());
    let other = AccountId::from_bytes([0x99; 16]);
    assert_eq!(
        verify_assertion(
            &own,
            &m.anchors,
            &recording,
            now(ISSUED),
            &m.one.rules,
            &other
        )
        .err(),
        Some(AssertError::WrongAccount)
    );
    assert!(recording.questions().is_empty());
    // ... and an expired assertion, or a forged one, asks nothing either.
    assert_eq!(
        verify_assertion(
            &own,
            &m.anchors,
            &recording,
            now(ISSUED + 300),
            &m.one.rules,
            &account()
        )
        .err(),
        Some(AssertError::Expired)
    );
    assert!(recording.questions().is_empty());
}

/// QUALITY rule 26 as everywhere in the crate: an id prints as its first four bytes and `..`.
#[test]
fn s9e3_an_installed_account_prints_four_bytes_of_its_account_and_never_the_whole_id() {
    let installed = InstalledAccount::new(account(), GROUP);
    let text = format!("{installed:?}");
    let id = account().to_string();
    assert!(!text.contains(&id), "a whole id in {text}");
    assert!(
        text.contains(&id[..8]),
        "the first four bytes of {id} in {text}"
    );
}

/// The alarm class (decision D-W1S9E3): exactly `GroupMismatch` of a credential and of an assertion and
/// `Revoked` of an entitlement. Each match below is exhaustive, so a new variant of an error enum does
/// not compile until its class is decided here.
#[test]
fn s9e3_exactly_three_refusals_are_alarms() {
    fn cred(e: CredError) -> bool {
        match e {
            CredError::GroupMismatch => true,
            CredError::ClockUntrusted
            | CredError::Malformed
            | CredError::LifetimeTooLong
            | CredError::CertMalformed
            | CredError::CertMismatch
            | CredError::NotYetValid
            | CredError::Expired
            | CredError::CertWindow
            | CredError::UnknownIdentity
            | CredError::IdentityWindow
            | CredError::BadCert
            | CredError::BadSignature
            | CredError::UnknownAccount
            | CredError::UnknownScope => false,
        }
    }
    fn assertion(e: AssertError) -> bool {
        match e {
            AssertError::GroupMismatch => true,
            AssertError::ClockUntrusted
            | AssertError::Malformed
            | AssertError::LifetimeTooLong
            | AssertError::WrongAccount
            | AssertError::CertMalformed
            | AssertError::NotYetValid
            | AssertError::Expired
            | AssertError::CertWindow
            | AssertError::UnknownIdentity
            | AssertError::IdentityWindow
            | AssertError::BadCert
            | AssertError::BadSignature
            | AssertError::UnknownAccount => false,
        }
    }
    fn entitlement(e: EntitlementError) -> bool {
        match e {
            EntitlementError::Revoked => true,
            EntitlementError::UnknownKey
            | EntitlementError::KeyWindow
            | EntitlementError::NotYetIssued
            | EntitlementError::BadSignature
            | EntitlementError::Malformed
            | EntitlementError::Sign(_)
            | EntitlementError::SignerFault => false,
        }
    }
    for e in [
        CredError::ClockUntrusted,
        CredError::Malformed,
        CredError::LifetimeTooLong,
        CredError::CertMalformed,
        CredError::CertMismatch,
        CredError::NotYetValid,
        CredError::Expired,
        CredError::CertWindow,
        CredError::UnknownIdentity,
        CredError::IdentityWindow,
        CredError::BadCert,
        CredError::BadSignature,
        CredError::UnknownAccount,
        CredError::GroupMismatch,
        CredError::UnknownScope,
    ] {
        assert_eq!(e.is_alarm(), cred(e), "{e:?}");
    }
    for e in [
        AssertError::ClockUntrusted,
        AssertError::Malformed,
        AssertError::LifetimeTooLong,
        AssertError::WrongAccount,
        AssertError::CertMalformed,
        AssertError::NotYetValid,
        AssertError::Expired,
        AssertError::CertWindow,
        AssertError::UnknownIdentity,
        AssertError::IdentityWindow,
        AssertError::BadCert,
        AssertError::BadSignature,
        AssertError::UnknownAccount,
        AssertError::GroupMismatch,
    ] {
        assert_eq!(e.is_alarm(), assertion(e), "{e:?}");
    }
    for e in [
        EntitlementError::UnknownKey,
        EntitlementError::KeyWindow,
        EntitlementError::NotYetIssued,
        EntitlementError::BadSignature,
        EntitlementError::Revoked,
        EntitlementError::Malformed,
        EntitlementError::Sign(azlin_creds::SignError::Refused),
        EntitlementError::SignerFault,
    ] {
        assert_eq!(e.is_alarm(), entitlement(e), "{e:?}");
    }
}
