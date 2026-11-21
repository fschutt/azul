//! The golden vectors of the member assertion and of the entitlement (the brief's S9E-14, the assertion
//! and entitlement half): `tests/vectors/member_assert_v1.json` and `tests/vectors/entitlement_v2.json`.
//! They pin the canonical bytes, the seeds and the signatures; a second test reads the FILES alone and
//! verifies every row through the crate's decoders and azlin-keys. INT-WIRE copies them to
//! azlin-contract's `fixtures/c4_*.json`. See `vectors.rs` for the format and `UPDATE_VECTORS`.

use azlin_creds::{
    sign_entitlement, verify_assertion_bytes, verify_entitlement, Assertion, AssertionClaims,
    Canonical, EntitlementClaims, SignedEntitlement, Signer, VerifyRules, ENTITLEMENT_V2,
    MEMBER_ASSERT_V1,
};
use azlin_keys::sign::verify;
use azlin_keys::{Secret, SignKeypair, Signature, Span};
use azlin_regbase::{
    AccountId, AccountKind, AccountRole, AnchorSet, DayStart, DeviceId, EntitlementState, Features,
    KeyWindow, MemberId, RegionId, RegionSet, SmallText, UserGroupId,
};
use azlin_seams::ByteCount;

use crate::support::*;
use crate::vectors::{check_or_update, parse_json, seed_of, seed_text, vector_path, Row};

const PAYMENTS_SEED: u8 = 41;
/// The day the vector entitlement is issued, as a day number, and the leader's stamp it is verified at.
const ISSUED_DAY: u64 = 20_000;
const VERIFIED_AT: u64 = ISSUED_DAY * 86_400 + 10;

fn assertion_claims() -> AssertionClaims {
    AssertionClaims {
        account: AccountId::from_bytes([0x11; 16]),
        member: MemberId::from_bytes([0x22; 16]),
        device: DeviceId::from_bytes([0x33; 16]),
        role: AccountRole::GroupAdmin,
        groups: vec![
            UserGroupId::from_bytes([0x01; 16]),
            UserGroupId::from_bytes([0x05; 16]),
            UserGroupId::from_bytes([0x09; 16]),
        ],
        issued: at(ISSUED),
        expires: at(ISSUED + 300),
    }
}

fn entitlement_claims() -> EntitlementClaims {
    let day = |n: u64| DayStart::from_start(at(n * 86_400)).expect("a day start");
    EntitlementClaims {
        account: AccountId::from_bytes([0x11; 16]),
        kind: AccountKind::Team,
        seats: 25,
        quota: ByteCount::gib(500),
        retention: Span::days(90),
        features: Features::new(
            ["audit", "sso", "team"]
                .iter()
                .map(|w| SmallText::new(w).expect("a word"))
                .collect(),
        )
        .expect("features"),
        period_until: Some(day(ISSUED_DAY + 30)),
        state: EntitlementState::Active,
        regions_allowed: RegionSet::new(vec![RegionId::new(1), RegionId::new(3)]).expect("regions"),
        issued_at: day(ISSUED_DAY),
        version: 7,
    }
}

fn payments_anchors() -> AnchorSet {
    AnchorSet {
        payments: vec![KeyWindow {
            public: signer(PAYMENTS_SEED).public(),
            not_before: at(0),
            not_after: None,
            revoked_from: None,
        }],
        operator: Vec::new(),
        identity: Vec::new(),
        map_epoch: 1,
    }
}

fn assertion_rows() -> Vec<Row> {
    let f = fixture();
    let claims = assertion_claims();
    let payload = claims.canonical_bytes().expect("payload");
    let sig = f
        .cred
        .sign(&MEMBER_ASSERT_V1, &payload)
        .expect("a signature");
    let assertion = Assertion::from_parts(claims.clone(), f.cert, sig).expect("an assertion");
    vec![
        Row::new(
            "assertion_admin_payload",
            "assertion_payload",
            "azlin-member-assert-v1",
            &to_hex(&payload),
            &seed_text(2),
            &to_hex(sig.as_bytes()),
        ),
        Row::new(
            "assertion_admin_wire",
            "assertion_wire",
            "",
            &to_hex(&assertion.encode().expect("wire")),
            &format!(
                "{} group=1 now={ISSUED} account={}",
                seed_text(1),
                claims.account
            ),
            "ok",
        ),
    ]
}

fn entitlement_rows() -> Vec<Row> {
    let payments = signer(PAYMENTS_SEED);
    let signed = sign_entitlement(&payments, entitlement_claims()).expect("signs");
    vec![
        Row::new(
            "entitlement_team_payload",
            "entitlement_payload",
            "azlin-entitlement-v2",
            &to_hex(signed.payload()),
            &seed_text(PAYMENTS_SEED),
            &to_hex(signed.signature().as_bytes()),
        ),
        Row::new(
            "entitlement_team_wire",
            "entitlement_wire",
            "",
            &to_hex(&signed.encode().expect("wire")),
            &format!("{} now={VERIFIED_AT}", seed_text(PAYMENTS_SEED)),
            "ok",
        ),
    ]
}

#[test]
fn s9e_14_the_assertion_vectors_are_what_the_crate_computes() {
    check_or_update("member_assert_v1.json", &assertion_rows());
}

#[test]
fn s9e_14_the_entitlement_vectors_are_what_the_crate_computes() {
    check_or_update("entitlement_v2.json", &entitlement_rows());
}

fn field_of<'a>(extra: &'a str, name: &str) -> &'a str {
    extra
        .split_whitespace()
        .find_map(|part| part.strip_prefix(name))
        .unwrap_or_else(|| panic!("{name} in the row"))
}

#[test]
fn s9e_14_the_assertion_and_entitlement_vector_files_verify_without_the_code_that_wrote_them() {
    for file in ["member_assert_v1.json", "entitlement_v2.json"] {
        let text = std::fs::read_to_string(vector_path(file)).unwrap_or_else(|e| {
            panic!("{file} is missing ({e}): UPDATE_VECTORS=1 writes it, never in CI")
        });
        let rows = parse_json(&text);
        assert!(!rows.is_empty());
        for row in rows {
            let bytes = from_hex(&row.hex);
            match row.kind.as_str() {
                "assertion_payload" | "entitlement_payload" => {
                    let domain = match row.context.as_str() {
                        "azlin-member-assert-v1" => MEMBER_ASSERT_V1,
                        "azlin-entitlement-v2" => ENTITLEMENT_V2,
                        other => panic!("{}: an unknown domain {other}", row.name),
                    };
                    let key = SignKeypair::from_seed(&Secret::from_bytes(seed_of(&row.extra)))
                        .verify_key();
                    let sig = Signature::from_bytes(
                        <[u8; 64]>::try_from(from_hex(&row.expect).as_slice()).expect("64 bytes"),
                    );
                    assert!(
                        verify(&domain, &bytes, &key, &sig).is_ok(),
                        "{}: the signature over domain || 0x00 || bytes",
                        row.name
                    );
                    if row.kind == "assertion_payload" {
                        let claims = AssertionClaims::decode_payload(&bytes).expect("claims");
                        assert_eq!(claims.canonical_bytes().expect("again"), bytes);
                    } else {
                        let claims = EntitlementClaims::decode_payload(&bytes).expect("claims");
                        assert_eq!(claims.canonical_bytes().expect("again"), bytes);
                    }
                }
                "assertion_wire" => {
                    let assertion = Assertion::decode(&bytes).expect("an assertion");
                    assert_eq!(assertion.encode().expect("again"), bytes, "{}", row.name);
                    let account = field_of(&row.extra, "account=")
                        .parse::<AccountId>()
                        .expect("an account id");
                    let anchors = fixture_anchors(GROUP, 1, &[(0, None)]);
                    verify_assertion_bytes(
                        &bytes,
                        &anchors,
                        &placement(),
                        now(ISSUED),
                        &VerifyRules::default(),
                        &account,
                    )
                    .unwrap_or_else(|e| panic!("{}: {e}", row.name));
                }
                "entitlement_wire" => {
                    let signed = SignedEntitlement::decode(&bytes).expect("an entitlement");
                    assert_eq!(signed.encode().expect("again"), bytes, "{}", row.name);
                    let stamp: u64 = field_of(&row.extra, "now=").parse().expect("a stamp");
                    verify_entitlement(&signed, &payments_anchors(), at(stamp))
                        .unwrap_or_else(|e| panic!("{}: {e}", row.name));
                }
                other => panic!("{}: an unknown kind {other}", row.name),
            }
        }
    }
}
