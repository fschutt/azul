//! S9E-07 `an_assertion_round_trips_and_expires_in_minutes` (planning/architecture.md section 5.11) and
//! the assertion halves of S9E-02, S9E-05, S9E-10 and S9E-18: the same chain as a credential, its own
//! lifetime, the installed account, groups that are sorted and distinct, and a probe before an issuer
//! exists.

use azlin_cbor::{IntMapBuilder, Value};
use azlin_creds::{
    verify_assertion, verify_assertion_bytes, AssertError, Assertion, AssertionClaims,
    AssertionIssuer, IssueError, IssuerError, ProbeFailure, Signer, VerifierNow, WireError,
    MAX_GROUPS,
};
use azlin_regbase::{AccountId, AccountRole, DeviceId, MemberId, UserGroupId};

use crate::support::*;

const ISSUED_A: u64 = ISSUED;
const TEN_MINUTES: u64 = 600;

fn account() -> AccountId {
    AccountId::from_bytes([0x11; 16])
}

fn group_id(n: u8) -> UserGroupId {
    UserGroupId::from_bytes([n; 16])
}

fn claims() -> AssertionClaims {
    AssertionClaims {
        account: account(),
        member: MemberId::from_bytes([0x22; 16]),
        device: DeviceId::from_bytes([0x33; 16]),
        role: AccountRole::Admin,
        groups: vec![group_id(1), group_id(5), group_id(9)],
        issued: at(ISSUED_A),
        expires: at(ISSUED_A + 300),
    }
}

fn issuer_of(f: &Fixture) -> AssertionIssuer<'_> {
    AssertionIssuer::new(&f.cred, f.cert, &f.anchors, at(ISSUED_A)).expect("the probe verifies")
}

fn verdict(f: &Fixture, a: &Assertion, secs: u64) -> Result<(), AssertError> {
    verify_assertion(a, &f.anchors, now(secs), &f.rules, &account()).map(|_| ())
}

/// An assertion of arbitrary claims, signed by the fixture's credential key.
fn signed(f: &Fixture, c: AssertionClaims) -> Assertion {
    use azlin_creds::{Canonical, MEMBER_ASSERT_V1};
    let payload = c.canonical_bytes().expect("canonical claims");
    let sig = f
        .cred
        .sign(&MEMBER_ASSERT_V1, &payload)
        .expect("a signature");
    Assertion::from_parts(c, f.cert, sig).expect("an assertion")
}

#[test]
fn s9e_07_an_assertion_round_trips() {
    let f = fixture();
    let issuer = issuer_of(&f);
    let assertion = issuer.issue(claims()).expect("an assertion");
    let verified = verify_assertion(&assertion, &f.anchors, now(ISSUED_A), &f.rules, &account())
        .expect("verifies");
    let c = claims();
    assert_eq!(verified.account(), c.account);
    assert_eq!(verified.member(), c.member);
    assert_eq!(verified.device(), c.device);
    assert_eq!(verified.role(), AccountRole::Admin);
    assert_eq!(verified.groups(), c.groups.as_slice());
    assert_eq!(verified.issued(), c.issued);
    assert_eq!(verified.expires(), c.expires);
    assert_eq!(verified.group(), GROUP);
    // The wire form is stable, and the bytes verify in one call.
    let bytes = assertion.encode().expect("encodes");
    let again = Assertion::decode(&bytes).expect("decodes");
    assert_eq!(again, assertion);
    assert_eq!(again.encode().expect("encodes again"), bytes);
    assert!(
        verify_assertion_bytes(&bytes, &f.anchors, now(ISSUED_A), &f.rules, &account()).is_ok()
    );
}

#[test]
fn s9e_07_an_assertion_lives_ten_minutes_at_most() {
    let f = fixture();
    let mut c = claims();
    c.expires = at(ISSUED_A + TEN_MINUTES);
    assert_eq!(verdict(&f, &signed(&f, c.clone()), ISSUED_A), Ok(()));
    c.expires = at(ISSUED_A + TEN_MINUTES + 1);
    assert_eq!(
        verdict(&f, &signed(&f, c.clone()), ISSUED_A),
        Err(AssertError::LifetimeTooLong)
    );
    c.expires = at(ISSUED_A);
    assert_eq!(
        verdict(&f, &signed(&f, c.clone()), ISSUED_A),
        Err(AssertError::Malformed)
    );
    c.expires = at(ISSUED_A - 1);
    assert_eq!(
        verdict(&f, &signed(&f, c), ISSUED_A),
        Err(AssertError::Malformed)
    );
    // The issuer refuses what the verifier would.
    let issuer = issuer_of(&f);
    let mut c = claims();
    c.expires = at(ISSUED_A + TEN_MINUTES + 1);
    assert_eq!(
        issuer.issue(c).err(),
        Some(IssueError::AssertionClaims(AssertError::LifetimeTooLong))
    );
}

#[test]
fn s9e_07_the_skew_and_the_expiry_are_a_credentials() {
    let f = fixture();
    let a = signed(&f, claims());
    let issued = ISSUED_A;
    let expires = ISSUED_A + 300;
    assert_eq!(verdict(&f, &a, issued - 31), Err(AssertError::NotYetValid));
    assert_eq!(verdict(&f, &a, issued - 30), Ok(()));
    assert_eq!(verdict(&f, &a, expires - 1), Ok(()));
    assert_eq!(verdict(&f, &a, expires), Err(AssertError::Expired));
    assert_eq!(verdict(&f, &a, expires + 600), Err(AssertError::Expired));
}

#[test]
fn s9e_07_an_assertion_for_another_account_is_refused() {
    let f = fixture();
    let a = signed(&f, claims());
    let other = AccountId::from_bytes([0x99; 16]);
    assert_eq!(
        verify_assertion(&a, &f.anchors, now(ISSUED_A), &f.rules, &other).err(),
        Some(AssertError::WrongAccount)
    );
    // The account is checked before any signature: a service installed for another account does no
    // cryptography for it.
    assert_eq!(
        verify_assertion_bytes(
            &a.encode().expect("encodes"),
            &f.anchors,
            now(ISSUED_A),
            &f.rules,
            &other
        )
        .err(),
        Some(AssertError::WrongAccount)
    );
}

#[test]
fn s9e_07_the_issuer_sorts_and_deduplicates_the_groups() {
    let f = fixture();
    let issuer = issuer_of(&f);
    let mut c = claims();
    c.groups = vec![
        group_id(9),
        group_id(1),
        group_id(9),
        group_id(5),
        group_id(1),
    ];
    let assertion = issuer.issue(c).expect("an assertion");
    assert_eq!(
        assertion.claims().groups,
        vec![group_id(1), group_id(5), group_id(9)]
    );
    // More distinct groups than the limit are refused, never truncated.
    let mut c = claims();
    c.groups = (0..=MAX_GROUPS)
        .map(|i| {
            let mut id = [0u8; 16];
            id[..8].copy_from_slice(&(i as u64).to_be_bytes());
            UserGroupId::from_bytes(id)
        })
        .collect();
    assert_eq!(c.groups.len(), MAX_GROUPS + 1);
    assert_eq!(
        issuer.issue(c).err(),
        Some(IssueError::AssertionClaims(AssertError::Malformed))
    );
}

/// The bytes of an assertion around an arbitrary list of groups, signed by the fixture's credential key.
fn craft(f: &Fixture, groups: Vec<Value>) -> Vec<u8> {
    use azlin_cbor::encode;
    use azlin_creds::MEMBER_ASSERT_V1;
    let c = claims();
    let payload = IntMapBuilder::new()
        .put(1, Value::Bytes(c.account.as_bytes().to_vec()))
        .put(2, Value::Bytes(c.member.as_bytes().to_vec()))
        .put(3, Value::Bytes(c.device.as_bytes().to_vec()))
        .put(4, Value::Text("member".to_string()))
        .put(5, Value::Array(groups))
        .put(6, Value::Uint(ISSUED_A))
        .put(7, Value::Uint(ISSUED_A + 300))
        .build()
        .expect("a payload");
    let payload_bytes = encode(&payload, &BIG).expect("the payload");
    let sig = f
        .cred
        .sign(&MEMBER_ASSERT_V1, &payload_bytes)
        .expect("a signature");
    let outer = IntMapBuilder::new()
        .put(1, Value::Bytes(payload_bytes))
        .put(2, f.cert.to_value().expect("the certificate"))
        .put(3, Value::Bytes(sig.as_bytes().to_vec()))
        .build()
        .expect("the wrapper");
    encode(&outer, &BIG).expect("the assertion")
}

fn id_value(n: u8) -> Value {
    Value::Bytes(vec![n; 16])
}

#[test]
fn s9e_07_a_verifier_refuses_groups_that_are_not_sorted_distinct_and_few() {
    let f = fixture();
    let ok = craft(&f, vec![id_value(1), id_value(2), id_value(3)]);
    assert!(
        verify_assertion_bytes(&ok, &f.anchors, now(ISSUED_A), &f.rules, &account()).is_ok(),
        "the control: a sorted list verifies"
    );
    for (name, groups, wire) in [
        (
            "unsorted",
            vec![id_value(2), id_value(1)],
            WireError::NotCanonical,
        ),
        (
            "duplicated",
            vec![id_value(1), id_value(1)],
            WireError::NotCanonical,
        ),
    ] {
        let bytes = craft(&f, groups);
        assert_eq!(Assertion::decode(&bytes).err(), Some(wire), "{name}");
        assert_eq!(
            verify_assertion_bytes(&bytes, &f.anchors, now(ISSUED_A), &f.rules, &account()).err(),
            Some(AssertError::Malformed),
            "{name}"
        );
    }
    // 257 groups: one too many.
    let many: Vec<Value> = (0..=MAX_GROUPS)
        .map(|i| {
            let mut id = vec![0u8; 16];
            id[..8].copy_from_slice(&(i as u64).to_be_bytes());
            Value::Bytes(id)
        })
        .collect();
    let bytes = craft(&f, many.clone());
    assert!(Assertion::decode(&bytes).is_err());
    assert_eq!(
        verify_assertion_bytes(&bytes, &f.anchors, now(ISSUED_A), &f.rules, &account()).err(),
        Some(AssertError::Malformed)
    );
    // 256 are fine; 10,000 are refused whatever they are.
    let bytes = craft(&f, many[..MAX_GROUPS].to_vec());
    assert!(
        verify_assertion_bytes(&bytes, &f.anchors, now(ISSUED_A), &f.rules, &account()).is_ok()
    );
    let huge: Vec<Value> = (0..10_000u64)
        .map(|i| {
            let mut id = vec![0u8; 16];
            id[..8].copy_from_slice(&i.to_be_bytes());
            Value::Bytes(id)
        })
        .collect();
    let bytes = craft(&f, huge);
    assert!(Assertion::decode(&bytes).is_err());
    // A group id of the wrong length.
    let bytes = craft(&f, vec![Value::Bytes(vec![1; 15])]);
    assert_eq!(Assertion::decode(&bytes).err(), Some(WireError::Malformed));
}

#[test]
fn s9e_07_an_assertions_role_is_a_word_of_its_vocabulary() {
    let f = fixture();
    for role in [
        AccountRole::Owner,
        AccountRole::Admin,
        AccountRole::GroupAdmin,
        AccountRole::Helpdesk,
        AccountRole::Member,
        AccountRole::Guest,
    ] {
        let mut c = claims();
        c.role = role;
        let a = signed(&f, c);
        let bytes = a.encode().expect("encodes");
        let verified =
            verify_assertion_bytes(&bytes, &f.anchors, now(ISSUED_A), &f.rules, &account())
                .expect("verifies");
        assert_eq!(verified.role(), role);
    }
}

#[test]
fn s9e_07_the_chain_and_the_clock_apply_to_assertions_too() {
    let f = fixture();
    let a = signed(&f, claims());
    // An untrusted clock decides nothing.
    for secs in [0, ISSUED_A, ISSUED_A + 1_000] {
        assert_eq!(
            verify_assertion(
                &a,
                &f.anchors,
                VerifierNow::untrusted(node(secs)),
                &f.rules,
                &account()
            )
            .err(),
            Some(AssertError::ClockUntrusted)
        );
    }
    // An empty anchor set refuses everything; another group's key does not vouch.
    assert_eq!(
        verify_assertion(
            &a,
            &azlin_regbase::AnchorSet::empty(),
            now(ISSUED_A),
            &f.rules,
            &account()
        )
        .err(),
        Some(AssertError::UnknownIdentity)
    );
    let other_group = fixture_anchors(azlin_regbase::MetaGroupId::new(2), 1, &[(0, None)]);
    assert_eq!(
        verify_assertion(&a, &other_group, now(ISSUED_A), &f.rules, &account()).err(),
        Some(AssertError::UnknownIdentity)
    );
    // A signature by another credential key.
    let mut forged = claims();
    forged.role = AccountRole::Owner;
    let payload = {
        use azlin_creds::Canonical;
        forged.canonical_bytes().expect("bytes")
    };
    let sig = signer(8)
        .sign(&azlin_creds::MEMBER_ASSERT_V1, &payload)
        .expect("a signature");
    let bad = Assertion::from_parts(forged, f.cert, sig).expect("an assertion");
    assert_eq!(verdict(&f, &bad, ISSUED_A), Err(AssertError::BadSignature));
    // A credential's signature is not an assertion's: the same payload cannot be moved between them.
    let (_, credential) = issue_fixture();
    let swapped = Assertion::from_parts(claims(), f.cert, *credential.signature()).expect("built");
    assert_eq!(
        verdict(&f, &swapped, ISSUED_A),
        Err(AssertError::BadSignature)
    );
}

#[test]
fn s9e_10_an_assertion_issuer_exists_only_after_the_probe_verified() {
    let f = fixture();
    assert!(AssertionIssuer::new(&f.cred, f.cert, &f.anchors, at(ISSUED_A)).is_ok());
    let failed = |e: AssertError| Some(IssuerError::ProbeFailed(ProbeFailure::Assertion(e)));
    // A window that does not cover the moment.
    assert_eq!(
        AssertionIssuer::new(&f.cred, f.cert, &f.anchors, at(CERT_FROM - 1)).err(),
        failed(AssertError::CertWindow)
    );
    // A certificate signed by an identity key the anchors do not know.
    let foreign = signed_cert(
        &signer(9),
        GROUP,
        EPOCH,
        f.cred.public(),
        CERT_FROM,
        CERT_FROM + CERT_SPAN,
    );
    assert_eq!(
        AssertionIssuer::new(&f.cred, foreign, &f.anchors, at(ISSUED_A)).err(),
        failed(AssertError::UnknownIdentity)
    );
    // A signer that is not the certified key.
    assert_eq!(
        AssertionIssuer::new(&signer(77), f.cert, &f.anchors, at(ISSUED_A)).err(),
        failed(AssertError::BadSignature)
    );
    // Anchors that know no identity key.
    assert_eq!(
        AssertionIssuer::new(
            &f.cred,
            f.cert,
            &azlin_regbase::AnchorSet::empty(),
            at(ISSUED_A)
        )
        .err(),
        failed(AssertError::UnknownIdentity)
    );
}

macro_rules! assertion_table {
    ($table:ident, $exhaustive:ident { $( $field:ident => |$c:ident| $body:expr ),+ $(,)? }) => {
        fn $table() -> Vec<(&'static str, fn(&mut AssertionClaims))> {
            vec![ $( (stringify!($field), (|$c: &mut AssertionClaims| { $body; }) as fn(&mut AssertionClaims)) ),+ ]
        }
        /// Adding a field to `AssertionClaims` without adding it to the table does not compile.
        #[allow(dead_code)]
        fn $exhaustive(v: AssertionClaims) {
            let AssertionClaims { $( $field: _ ),+ } = v;
        }
    };
}

assertion_table!(assertion_mutations, assertion_fields_are_all_listed {
    account => |c| c.account = AccountId::from_bytes([0xEE; 16]),
    member => |c| c.member = MemberId::from_bytes([0xEE; 16]),
    device => |c| c.device = DeviceId::from_bytes([0xEE; 16]),
    role => |c| c.role = AccountRole::Owner,
    groups => |c| c.groups.push(UserGroupId::from_bytes([0xEE; 16])),
    issued => |c| c.issued = at(c.issued.as_secs() + 1),
    expires => |c| c.expires = at(c.expires.as_secs() + 1),
});

#[test]
fn s9e_02_any_changed_assertion_field_breaks_the_signature() {
    let f = fixture();
    let original = signed(&f, claims());
    assert_eq!(
        verdict(&f, &original, ISSUED_A),
        Ok(()),
        "the control verifies"
    );
    let mutations = assertion_mutations();
    assert_eq!(mutations.len(), 7, "the seven fields of 5.11");
    for (field, mutate) in mutations {
        let mut changed = claims();
        mutate(&mut changed);
        assert_ne!(changed, claims(), "{field}: the mutation changes nothing");
        let installed = changed.account;
        let forged = Assertion::from_parts(changed, f.cert, *original.signature())
            .expect("the changed claims encode");
        // The service is installed for the CHANGED account, so that the account check does not hide
        // the signature check for that one field.
        assert_eq!(
            verify_assertion(&forged, &f.anchors, now(ISSUED_A), &f.rules, &installed).err(),
            Some(AssertError::BadSignature),
            "{field} changed"
        );
    }
}

#[test]
fn s9e_08_an_assertions_signature_is_for_its_domain_only() {
    use azlin_creds::{Canonical, ALL_DOMAINS, MEMBER_ASSERT_V1};
    use azlin_keys::sign::verify;
    let f = fixture();
    let a = signed(&f, claims());
    let payload = claims().canonical_bytes().expect("bytes");
    assert_eq!(payload, a.payload());
    for domain in ALL_DOMAINS {
        let ok = verify(domain, &payload, &f.cert.cred_pub, a.signature()).is_ok();
        assert_eq!(ok, *domain == MEMBER_ASSERT_V1, "under {domain:?}");
    }
    assert_eq!(<AssertionClaims as Canonical>::DOMAIN, MEMBER_ASSERT_V1);
}
