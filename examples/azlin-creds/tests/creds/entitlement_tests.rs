//! S9E-09 `an_entitlement_is_valid_for_the_key_of_its_day` [M] (planning/architecture.md section 2.4,
//! COMMANDS2 `keys rotate payments`: "entitlements signed by the old key stay valid until they expire"),
//! the entitlement half of S9E-02, S9E-08 and S9E-18, and the guards that keep the rule from being a
//! hole: a key staged for the future cannot sign into its window, and the verified value hands the
//! registry the key and the day.

use azlin_cbor::{encode, IntMapBuilder, Value};
use azlin_creds::{
    sign_entitlement, verify_entitlement, EntitlementClaims, EntitlementError, SignError,
    SignedEntitlement, Signer, WireError, ENTITLEMENT_V2, ISSUE_SKEW,
};
use azlin_regbase::{
    AccountId, AccountKind, AnchorSet, DayStart, EntitlementState, Features, KeyWindow, LeaderTime,
    RegionId, RegionSet, SmallText,
};
use azlin_seams::ByteCount;

use crate::support::*;

const DAY: u64 = 86_400;

fn day(n: u64) -> DayStart {
    DayStart::from_start(at(n * DAY)).expect("a day start")
}

fn features(words: &[&str]) -> Features {
    Features::new(
        words
            .iter()
            .map(|w| SmallText::new(w).expect("a word"))
            .collect(),
    )
    .expect("features")
}

fn claims(issued_day: u64) -> EntitlementClaims {
    EntitlementClaims {
        account: AccountId::from_bytes([0x11; 16]),
        kind: AccountKind::Team,
        seats: 25,
        quota: ByteCount::gib(500),
        retention: azlin_keys::Span::days(90),
        features: features(&["audit", "sso", "team"]),
        period_until: Some(day(issued_day + 30)),
        state: EntitlementState::Active,
        regions_allowed: RegionSet::new(vec![RegionId::new(1), RegionId::new(3)]).expect("regions"),
        issued_at: day(issued_day),
        version: 7,
    }
}

/// Anchors of a payments-key rotation at day D = 100: the old key A (seed 41) until D, the new key B
/// (seed 42) from D.
fn rotated() -> AnchorSet {
    let a = signer(41);
    let b = signer(42);
    AnchorSet {
        payments: vec![
            KeyWindow {
                public: a.public(),
                not_before: at(0),
                not_after: Some(at(100 * DAY)),
            },
            KeyWindow {
                public: b.public(),
                not_before: at(100 * DAY),
                not_after: None,
            },
        ],
        operator: Vec::new(),
        identity: Vec::new(),
        map_epoch: 3,
    }
}

fn after(day_number: u64) -> LeaderTime {
    at(day_number * DAY + 10)
}

#[test]
fn s9e_09_an_entitlement_round_trips_and_verifies() {
    let a = signer(41);
    let signed = sign_entitlement(&a, claims(90)).expect("signs");
    let verified = verify_entitlement(&signed, &rotated(), after(90)).expect("verifies");
    let c = claims(90);
    assert_eq!(verified.account(), c.account);
    assert_eq!(verified.kind(), c.kind);
    assert_eq!(verified.seats(), c.seats);
    assert_eq!(verified.quota(), c.quota);
    assert_eq!(verified.retention(), c.retention);
    assert_eq!(verified.features(), &c.features);
    assert_eq!(verified.period_until(), c.period_until);
    assert_eq!(verified.state(), c.state);
    assert_eq!(verified.regions_allowed(), &c.regions_allowed);
    assert_eq!(verified.issued_at(), c.issued_at);
    assert_eq!(verified.version(), 7);
    assert_eq!(verified.key(), &a.public());
    // The bytes the registry stores round trip, and read back verify.
    let bytes = signed.encode().expect("encodes");
    let again = SignedEntitlement::decode(&bytes).expect("decodes");
    assert_eq!(again, signed);
    assert_eq!(again.encode().expect("again"), bytes);
    assert!(verify_entitlement(&again, &rotated(), after(90)).is_ok());
}

#[test]
fn s9e_09_the_old_key_signs_for_the_days_before_its_window_ends() {
    let a = signer(41);
    // Signed by A on day 99 (the last day of its window): verifies well AFTER the rotation: the key is
    // tested at the day of issue, not at the moment of verification.
    let signed = sign_entitlement(&a, claims(99)).expect("signs");
    for now_day in [99, 100, 101, 130, 400] {
        assert!(
            verify_entitlement(&signed, &rotated(), after(now_day)).is_ok(),
            "verified on day {now_day}"
        );
    }
    // Signed by A with `issued_at` on day 100 or later: the window of A is over.
    for issued_day in [100, 101, 150] {
        let late = sign_entitlement(&a, claims(issued_day)).expect("signs");
        assert_eq!(
            verify_entitlement(&late, &rotated(), after(issued_day)).err(),
            Some(EntitlementError::KeyWindow),
            "A on day {issued_day}"
        );
    }
}

#[test]
fn s9e_09_the_new_key_signs_from_its_day_on_and_not_before() {
    let b = signer(42);
    let on_day = sign_entitlement(&b, claims(100)).expect("signs");
    assert!(verify_entitlement(&on_day, &rotated(), after(100)).is_ok());
    let before = sign_entitlement(&b, claims(99)).expect("signs");
    assert_eq!(
        verify_entitlement(&before, &rotated(), after(99)).err(),
        Some(EntitlementError::KeyWindow),
        "B has no window on day 99"
    );
    // A staged key cannot sign into its future window: dated day 100, offered on day 99.
    assert_eq!(
        verify_entitlement(&on_day, &rotated(), after(99)).err(),
        Some(EntitlementError::NotYetIssued)
    );
    // The skew: 30 s before the day begins is fine, 31 s is not.
    let day_100 = 100 * DAY;
    assert!(verify_entitlement(&on_day, &rotated(), at(day_100 - ISSUE_SKEW.as_secs())).is_ok());
    assert_eq!(
        verify_entitlement(&on_day, &rotated(), at(day_100 - ISSUE_SKEW.as_secs() - 1)).err(),
        Some(EntitlementError::NotYetIssued)
    );
}

#[test]
fn s9e_09_an_unknown_key_and_a_revoked_key_are_refused() {
    let unknown = signer(99);
    let signed = sign_entitlement(&unknown, claims(50)).expect("signs");
    assert_eq!(
        verify_entitlement(&signed, &rotated(), after(50)).err(),
        Some(EntitlementError::UnknownKey)
    );
    // A key whose window closed before the issue day (revoked): KeyWindow.
    let c = signer(43);
    let mut anchors = rotated();
    anchors.payments.push(KeyWindow {
        public: c.public(),
        not_before: at(0),
        not_after: Some(at(40 * DAY)),
    });
    let late = sign_entitlement(&c, claims(41)).expect("signs");
    assert_eq!(
        verify_entitlement(&late, &anchors, after(41)).err(),
        Some(EntitlementError::KeyWindow)
    );
    let early = sign_entitlement(&c, claims(39)).expect("signs");
    assert!(verify_entitlement(&early, &anchors, after(41)).is_ok());
    // An empty anchor set refuses everything.
    assert_eq!(
        verify_entitlement(&early, &AnchorSet::empty(), after(41)).err(),
        Some(EntitlementError::UnknownKey)
    );
    // An identity key is no payments key.
    let mut identity_only = AnchorSet::empty();
    identity_only.identity.push((
        GROUP,
        KeyWindow {
            public: signer(41).public(),
            not_before: at(0),
            not_after: None,
        },
    ));
    assert_eq!(
        verify_entitlement(&early, &identity_only, after(41)).err(),
        Some(EntitlementError::UnknownKey)
    );
}

#[test]
fn s9e_09_a_signature_by_another_key_than_the_one_named_is_refused() {
    let a = signer(41);
    let b = signer(42);
    let signed = sign_entitlement(&a, claims(99)).expect("signs");
    // The same claims and signature, claiming to be B's: B has a window then? On day 99 it does not.
    let as_b =
        SignedEntitlement::from_parts(claims(99), b.public(), *signed.signature()).expect("built");
    assert_eq!(
        verify_entitlement(&as_b, &rotated(), after(99)).err(),
        Some(EntitlementError::KeyWindow)
    );
    // On day 100 B has a window, and A's signature is not B's.
    let a_on_100 = sign_entitlement(&a, claims(100)).expect("signs");
    let as_b = SignedEntitlement::from_parts(claims(100), b.public(), *a_on_100.signature())
        .expect("built");
    assert_eq!(
        verify_entitlement(&as_b, &rotated(), after(100)).err(),
        Some(EntitlementError::BadSignature)
    );
}

macro_rules! entitlement_table {
    ($table:ident, $exhaustive:ident { $( $field:ident => |$c:ident| $body:expr ),+ $(,)? }) => {
        fn $table() -> Vec<(&'static str, fn(&mut EntitlementClaims))> {
            vec![ $( (stringify!($field), (|$c: &mut EntitlementClaims| { $body; }) as fn(&mut EntitlementClaims)) ),+ ]
        }
        /// Adding a field to `EntitlementClaims` without adding it to the table does not compile.
        #[allow(dead_code)]
        fn $exhaustive(v: EntitlementClaims) {
            let EntitlementClaims { $( $field: _ ),+ } = v;
        }
    };
}

entitlement_table!(entitlement_mutations, entitlement_fields_are_all_listed {
    account => |c| c.account = AccountId::from_bytes([0xEE; 16]),
    kind => |c| c.kind = AccountKind::Business,
    seats => |c| c.seats += 1,
    quota => |c| c.quota = ByteCount::gib(501),
    retention => |c| c.retention = azlin_keys::Span::days(91),
    features => |c| c.features = features(&["audit", "sso", "team", "escrow"]),
    period_until => |c| c.period_until = if c.period_until.is_some() { None } else { Some(day(200)) },
    state => |c| c.state = EntitlementState::Lapsed,
    regions_allowed => |c| c.regions_allowed = RegionSet::new(vec![RegionId::new(1)]).expect("regions"),
    issued_at => |c| c.issued_at = day(91),
    version => |c| c.version += 1,
});

#[test]
fn s9e_02_any_changed_entitlement_field_breaks_the_signature() {
    let a = signer(41);
    let signed = sign_entitlement(&a, claims(90)).expect("signs");
    let mutations = entitlement_mutations();
    assert_eq!(mutations.len(), 11, "the eleven fields of 5.11 and D-9");
    verify_entitlement(&signed, &rotated(), after(95)).expect("the control verifies");
    for (field, mutate) in mutations {
        let mut changed = claims(90);
        mutate(&mut changed);
        assert_ne!(changed, claims(90), "{field}: the mutation changes nothing");
        let forged = SignedEntitlement::from_parts(changed, a.public(), *signed.signature())
            .expect("the changed claims encode");
        assert_eq!(
            verify_entitlement(&forged, &rotated(), after(95)).err(),
            Some(EntitlementError::BadSignature),
            "{field} changed"
        );
    }
}

#[test]
fn s9e_08_an_entitlements_signature_is_for_its_domain_only() {
    use azlin_creds::{Canonical, ALL_DOMAINS};
    use azlin_keys::sign::verify;
    let a = signer(41);
    let signed = sign_entitlement(&a, claims(90)).expect("signs");
    let payload = claims(90).canonical_bytes().expect("bytes");
    assert_eq!(payload, signed.payload());
    for domain in ALL_DOMAINS {
        let ok = verify(domain, &payload, &a.public(), signed.signature()).is_ok();
        assert_eq!(ok, *domain == ENTITLEMENT_V2, "under {domain:?}");
    }
    assert_eq!(<EntitlementClaims as Canonical>::DOMAIN, ENTITLEMENT_V2);
    // The signature of a credential's or an assertion's domain over the same bytes is not an entitlement's.
    for other in [
        azlin_creds::CREDENTIAL_V2,
        azlin_creds::MEMBER_ASSERT_V1,
        azlin_creds::CREDENTIAL_CERT_V1,
    ] {
        let sig = a.sign(&other, &payload).expect("signs");
        let forged = SignedEntitlement::from_parts(claims(90), a.public(), sig).expect("built");
        assert_eq!(
            verify_entitlement(&forged, &rotated(), after(95)).err(),
            Some(EntitlementError::BadSignature),
            "signed under {other:?}"
        );
    }
}

#[test]
fn s9e_09_the_retention_is_a_whole_number_of_days_and_the_signer_is_checked() {
    let a = signer(41);
    let mut c = claims(90);
    c.retention = azlin_keys::Span::secs(DAY + 1);
    assert_eq!(
        sign_entitlement(&a, c).err(),
        Some(EntitlementError::Malformed),
        "a retention of a day and a second"
    );
    // A holder that refuses.
    struct Refusing(azlin_creds::SeedSigner);
    impl Signer for Refusing {
        fn public(&self) -> azlin_regbase::PublicKey32 {
            self.0.public()
        }
        fn sign(
            &self,
            _: &azlin_regbase::Domain,
            _: &[u8],
        ) -> Result<azlin_regbase::Signature64, SignError> {
            Err(SignError::Unavailable)
        }
    }
    assert_eq!(
        sign_entitlement(&Refusing(signer(41)), claims(90)).err(),
        Some(EntitlementError::Sign(SignError::Unavailable))
    );
    // A holder whose signature is of something else.
    struct Garbage(azlin_creds::SeedSigner);
    impl Signer for Garbage {
        fn public(&self) -> azlin_regbase::PublicKey32 {
            self.0.public()
        }
        fn sign(
            &self,
            domain: &azlin_regbase::Domain,
            _: &[u8],
        ) -> Result<azlin_regbase::Signature64, SignError> {
            self.0.sign(domain, b"not what was asked")
        }
    }
    assert_eq!(
        sign_entitlement(&Garbage(signer(41)), claims(90)).err(),
        Some(EntitlementError::SignerFault)
    );
}

/// The payload map of an entitlement's claims, key by key as the crate writes it.
fn payload_of(c: &EntitlementClaims) -> IntMapBuilder {
    let mut b = IntMapBuilder::new()
        .put(1, Value::Bytes(c.account.as_bytes().to_vec()))
        .put(2, Value::Text(c.kind.as_str().to_string()))
        .put(3, Value::Uint(u64::from(c.seats)))
        .put(4, Value::Uint(c.quota.get()))
        .put(5, Value::Uint(c.retention.as_secs() / DAY))
        .put(8, Value::Text(c.state.as_str().to_string()))
        .put(10, Value::Uint(c.issued_at.start().as_secs()))
        .put(11, Value::Uint(c.version));
    if let Some(until) = c.period_until {
        b = b.put(7, Value::Uint(until.start().as_secs()));
    }
    b
}

/// The bytes of a signed entitlement around arbitrary features and regions values, signed by key A.
fn craft(
    features: Value,
    regions: Value,
    extra: impl Fn(IntMapBuilder) -> IntMapBuilder,
) -> Vec<u8> {
    let a = signer(41);
    let c = claims(90);
    let payload = extra(payload_of(&c).put(6, features).put(9, regions))
        .build()
        .expect("a payload");
    let payload_bytes = encode(&payload, &BIG).expect("the payload");
    let sig = a
        .sign(&ENTITLEMENT_V2, &payload_bytes)
        .expect("a signature");
    let outer = IntMapBuilder::new()
        .put(1, Value::Bytes(payload_bytes))
        .put(2, Value::Bytes(a.public().as_bytes().to_vec()))
        .put(3, Value::Bytes(sig.as_bytes().to_vec()))
        .build()
        .expect("the wrapper");
    encode(&outer, &BIG).expect("the entitlement")
}

fn text(t: &str) -> Value {
    Value::Text(t.to_string())
}

#[test]
fn s9e_18_hostile_entitlements_are_refused_never_repaired() {
    let words = |ws: &[&str]| Value::Array(ws.iter().map(|w| text(w)).collect());
    let regions = |rs: &[u64]| Value::Array(rs.iter().map(|r| Value::Uint(*r)).collect());
    let none = |b: IntMapBuilder| b;
    // The control: sorted features and regions decode.
    let ok = craft(words(&["audit", "team"]), regions(&[1, 3]), none);
    assert!(SignedEntitlement::decode(&ok).is_ok());
    // Unsorted, duplicated, not machine words, too many.
    assert_eq!(
        SignedEntitlement::decode(&craft(words(&["team", "audit"]), regions(&[1]), none)).err(),
        Some(WireError::NotCanonical)
    );
    assert_eq!(
        SignedEntitlement::decode(&craft(words(&["audit", "audit"]), regions(&[1]), none)).err(),
        Some(WireError::NotCanonical)
    );
    assert_eq!(
        SignedEntitlement::decode(&craft(words(&["Audit"]), regions(&[1]), none)).err(),
        Some(WireError::Malformed),
        "a feature is a lower-case machine word"
    );
    let many: Vec<String> = (0..33).map(|i| format!("feature_{i:02}")).collect();
    let many_words = Value::Array(many.iter().map(|w| text(w)).collect());
    assert_eq!(
        SignedEntitlement::decode(&craft(many_words, regions(&[1]), none)).err(),
        Some(WireError::TooMany)
    );
    assert_eq!(
        SignedEntitlement::decode(&craft(words(&["audit"]), regions(&[3, 1]), none)).err(),
        Some(WireError::NotCanonical)
    );
    assert_eq!(
        SignedEntitlement::decode(&craft(words(&["audit"]), regions(&[1, 1]), none)).err(),
        Some(WireError::NotCanonical)
    );
    assert_eq!(
        SignedEntitlement::decode(&craft(words(&["audit"]), regions(&[70_000]), none)).err(),
        Some(WireError::OutOfRange(9))
    );
    // A feature that is not text, a region that is not a number.
    assert_eq!(
        SignedEntitlement::decode(&craft(
            Value::Array(vec![Value::Uint(1)]),
            regions(&[1]),
            none
        ))
        .err(),
        Some(WireError::Malformed)
    );
    // A period that is not a day start is refused, not rounded.
    let not_a_day = |b: IntMapBuilder| b.put(7, Value::Uint(90 * DAY + 5));
    let c = claims(90);
    let mut without_period = c.clone();
    without_period.period_until = None;
    let a = signer(41);
    let payload = not_a_day(
        payload_of(&without_period)
            .put(6, words(&["audit"]))
            .put(9, regions(&[1])),
    )
    .build()
    .expect("a payload");
    let payload_bytes = encode(&payload, &BIG).expect("the payload");
    let sig = a
        .sign(&ENTITLEMENT_V2, &payload_bytes)
        .expect("a signature");
    let outer = IntMapBuilder::new()
        .put(1, Value::Bytes(payload_bytes))
        .put(2, Value::Bytes(a.public().as_bytes().to_vec()))
        .put(3, Value::Bytes(sig.as_bytes().to_vec()))
        .build()
        .expect("the wrapper");
    let bytes = encode(&outer, &BIG).expect("the entitlement");
    assert_eq!(
        SignedEntitlement::decode(&bytes).err(),
        Some(WireError::OutOfRange(7))
    );
    // An unknown state word and an unknown kind word.
    let bad_state = |b: IntMapBuilder| b.put(40, Value::Uint(1));
    let ok_extra = craft(words(&["audit"]), regions(&[1]), bad_state);
    assert!(
        SignedEntitlement::decode(&ok_extra).is_ok(),
        "an unknown key is ignored"
    );
    for (key, word) in [(8u64, "past_due"), (2, "enterprise")] {
        let mut payload = IntMapBuilder::new()
            .put(1, Value::Bytes(c.account.as_bytes().to_vec()))
            .put(3, Value::Uint(1))
            .put(4, Value::Uint(1))
            .put(5, Value::Uint(1))
            .put(6, words(&["audit"]))
            .put(9, regions(&[1]))
            .put(10, Value::Uint(90 * DAY))
            .put(11, Value::Uint(1));
        payload = if key == 8 {
            payload.put(2, text("team")).put(8, text(word))
        } else {
            payload.put(2, text(word)).put(8, text("active"))
        };
        let payload_bytes = encode(&payload.build().expect("a payload"), &BIG).expect("bytes");
        let sig = a
            .sign(&ENTITLEMENT_V2, &payload_bytes)
            .expect("a signature");
        let outer = IntMapBuilder::new()
            .put(1, Value::Bytes(payload_bytes))
            .put(2, Value::Bytes(a.public().as_bytes().to_vec()))
            .put(3, Value::Bytes(sig.as_bytes().to_vec()))
            .build()
            .expect("the wrapper");
        let bytes = encode(&outer, &BIG).expect("the entitlement");
        assert_eq!(
            SignedEntitlement::decode(&bytes).err(),
            Some(WireError::UnknownWord),
            "{word}"
        );
    }
}

#[test]
fn s9e_18_an_unknown_entitlement_field_is_ignored_and_the_signature_still_verifies() {
    let a = signer(41);
    let c = claims(90);
    let payload = payload_of(&c)
        .put(
            6,
            Value::Array(vec![text("audit"), text("sso"), text("team")]),
        )
        .put(9, Value::Array(vec![Value::Uint(1), Value::Uint(3)]))
        .put(60, text("a field of a newer payments service"))
        .build()
        .expect("a payload");
    let payload_bytes = encode(&payload, &BIG).expect("the payload");
    let sig = a
        .sign(&ENTITLEMENT_V2, &payload_bytes)
        .expect("a signature");
    let outer = IntMapBuilder::new()
        .put(1, Value::Bytes(payload_bytes))
        .put(2, Value::Bytes(a.public().as_bytes().to_vec()))
        .put(3, Value::Bytes(sig.as_bytes().to_vec()))
        .build()
        .expect("the wrapper");
    let bytes = encode(&outer, &BIG).expect("the entitlement");
    let signed = SignedEntitlement::decode(&bytes).expect("decodes");
    assert!(verify_entitlement(&signed, &rotated(), after(95)).is_ok());
    assert_eq!(
        signed.encode().expect("encodes"),
        bytes,
        "the registry keeps the signed bytes"
    );
}
