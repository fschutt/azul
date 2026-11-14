//! The refresh frames (patch P-S9e-4; planning/architecture.md section 2.4, 5.3, DRIFT X-D-07): the request
//! (a family token, or a key proof over the node's challenge) and the answer (a grant or a refusal),
//! their strict canonical CBOR, the secrets that never print, the vectors `refresh_request.json` and
//! `refresh_answer.json`.

use azlin_cbor::{encode, IntMapBuilder, Value};
use azlin_creds::{
    verify_credential_bytes, CanonicalError, Credential, FamilyToken, RefreshAnswerWire,
    RefreshProof, RefreshRequest, RefreshSubject, WireError, CREDENTIAL_MAX, MAX_NOTICES,
};
use azlin_keys::Secret;
use azlin_regbase::{
    AccountId, Challenge, ClientVersion, DayStart, DeviceId, FamilyId, KeyProof, MemberId,
    RefusalWord, Signature64, SmallText,
};

use crate::support::*;
use crate::vectors::{check_or_update, parse_json, vector_path, Row};

fn account() -> AccountId {
    AccountId::from_bytes([0x11; 16])
}

fn device() -> DeviceId {
    DeviceId::from_bytes([0x33; 16])
}

fn app() -> MemberId {
    MemberId::from_bytes([0x77; 16])
}

fn token() -> FamilyToken {
    FamilyToken::new(
        FamilyId::from_bytes([0x44; 16]),
        Secret::from_bytes([0x07; 32]),
    )
}

fn key_proof() -> KeyProof {
    KeyProof {
        challenge: Challenge::from_bytes([0x0c; 16]),
        signature: Signature64::from_bytes([0x5a; 64]),
    }
}

fn request(subject: RefreshSubject, proof: RefreshProof) -> RefreshRequest {
    RefreshRequest {
        account: account(),
        subject,
        proof,
        client: ClientVersion::new(3, 14, 159),
    }
}

fn same_proof(a: &RefreshProof, b: &RefreshProof) -> bool {
    match (a, b) {
        (RefreshProof::Token(x), RefreshProof::Token(y)) => {
            x.family() == y.family() && x.secret().ct_eq(y.secret())
        }
        (RefreshProof::Key(x), RefreshProof::Key(y)) => x == y,
        _ => false,
    }
}

fn same_request(a: &RefreshRequest, b: &RefreshRequest) -> bool {
    a.account == b.account
        && a.subject == b.subject
        && a.client == b.client
        && same_proof(&a.proof, &b.proof)
}

fn valid_requests() -> Vec<(&'static str, RefreshRequest)> {
    vec![
        (
            "device_token",
            request(
                RefreshSubject::Device(device()),
                RefreshProof::Token(token()),
            ),
        ),
        (
            "device_key",
            request(
                RefreshSubject::Device(device()),
                RefreshProof::Key(key_proof()),
            ),
        ),
        (
            "app_key",
            request(RefreshSubject::App(app()), RefreshProof::Key(key_proof())),
        ),
    ]
}

#[test]
fn s9e_refresh_a_request_round_trips_byte_for_byte() {
    for (name, original) in valid_requests() {
        let bytes = original.encode().expect("encodes");
        let decoded = RefreshRequest::decode(&bytes).expect("decodes");
        assert!(same_request(&original, &decoded), "{name}");
        assert_eq!(
            decoded.encode().expect("again").as_slice(),
            bytes.as_slice(),
            "{name}"
        );
    }
}

#[test]
fn s9e_refresh_the_writer_is_the_codecs_canonical_form() {
    // The hand-written writer (the secret never passes through a `Value`) must write exactly what the
    // strict codec writes for the same map, in the shortest forms, for numbers of every width.
    let mut r = request(
        RefreshSubject::Device(device()),
        RefreshProof::Token(token()),
    );
    for version in [
        ClientVersion::new(0, 0, 0),
        ClientVersion::new(23, 24, 255),
        ClientVersion::new(256, 1, 65_535),
    ] {
        r.client = version;
        let written = r.encode().expect("encodes");
        let map = IntMapBuilder::new()
            .put(1, Value::Bytes(account().as_bytes().to_vec()))
            .put(2, Value::Uint(1))
            .put(3, Value::Bytes(device().as_bytes().to_vec()))
            .put(4, Value::Uint(1))
            .put(5, Value::Bytes(vec![0x44; 16]))
            .put(6, Value::Bytes(vec![0x07; 32]))
            .put(
                9,
                Value::Array(vec![
                    Value::Uint(u64::from(version.major())),
                    Value::Uint(u64::from(version.minor())),
                    Value::Uint(u64::from(version.patch())),
                ]),
            )
            .build()
            .expect("a map");
        assert_eq!(
            written.as_slice(),
            encode(&map, &BIG).expect("encodes").as_slice(),
            "{version}"
        );
    }
    let key = request(RefreshSubject::App(app()), RefreshProof::Key(key_proof()));
    let map = IntMapBuilder::new()
        .put(1, Value::Bytes(account().as_bytes().to_vec()))
        .put(2, Value::Uint(2))
        .put(3, Value::Bytes(app().as_bytes().to_vec()))
        .put(4, Value::Uint(2))
        .put(7, Value::Bytes(vec![0x0c; 16]))
        .put(8, Value::Bytes(vec![0x5a; 64]))
        .put(
            9,
            Value::Array(vec![Value::Uint(3), Value::Uint(14), Value::Uint(159)]),
        )
        .build()
        .expect("a map");
    assert_eq!(
        key.encode().expect("encodes").as_slice(),
        encode(&map, &BIG).expect("encodes").as_slice()
    );
}

#[test]
fn s9e_refresh_an_app_principal_has_no_token() {
    let bad = request(RefreshSubject::App(app()), RefreshProof::Token(token()));
    assert_eq!(bad.encode().err(), Some(CanonicalError::NotCanonical));
    // The same on the wire.
    let map = request_map(2, 1, Some(vec![0x44; 16]), Some(vec![0x07; 32]), None, None);
    assert_eq!(
        RefreshRequest::decode(&map).err(),
        Some(WireError::Malformed)
    );
}

/// A request map by hand: the subject kind, the proof kind and whichever of the proof's fields.
fn request_map(
    subject_kind: u64,
    proof_kind: u64,
    family: Option<Vec<u8>>,
    secret: Option<Vec<u8>>,
    challenge: Option<Vec<u8>>,
    signature: Option<Vec<u8>>,
) -> Vec<u8> {
    let mut b = IntMapBuilder::new()
        .put(1, Value::Bytes(account().as_bytes().to_vec()))
        .put(2, Value::Uint(subject_kind))
        .put(3, Value::Bytes(device().as_bytes().to_vec()))
        .put(4, Value::Uint(proof_kind))
        .put(
            9,
            Value::Array(vec![Value::Uint(1), Value::Uint(2), Value::Uint(3)]),
        );
    for (key, value) in [(5, family), (6, secret), (7, challenge), (8, signature)] {
        if let Some(bytes) = value {
            b = b.put(key, Value::Bytes(bytes));
        }
    }
    encode(&b.build().expect("a map"), &BIG).expect("encodes")
}

#[test]
fn s9e_refresh_exactly_one_proof_with_all_its_fields() {
    let fam = || Some(vec![0x44; 16]);
    let sec = || Some(vec![0x07; 32]);
    let chal = || Some(vec![0x0c; 16]);
    let sig = || Some(vec![0x5a; 64]);
    // The controls.
    assert!(RefreshRequest::decode(&request_map(1, 1, fam(), sec(), None, None)).is_ok());
    assert!(RefreshRequest::decode(&request_map(1, 2, None, None, chal(), sig())).is_ok());
    for (name, bytes) in [
        (
            "both proofs",
            request_map(1, 1, fam(), sec(), chal(), sig()),
        ),
        (
            "a token without its secret",
            request_map(1, 1, fam(), None, None, None),
        ),
        (
            "a token without its family",
            request_map(1, 1, None, sec(), None, None),
        ),
        (
            "a key proof without its signature",
            request_map(1, 2, None, None, chal(), None),
        ),
        (
            "a key proof without its challenge",
            request_map(1, 2, None, None, None, sig()),
        ),
        ("no proof fields", request_map(1, 1, None, None, None, None)),
        (
            "the token's fields under a key proof",
            request_map(1, 2, fam(), sec(), None, None),
        ),
        (
            "an unknown proof kind",
            request_map(1, 3, fam(), sec(), None, None),
        ),
        (
            "an unknown subject kind",
            request_map(3, 1, fam(), sec(), None, None),
        ),
        (
            "a short secret",
            request_map(1, 1, fam(), Some(vec![0x07; 31]), None, None),
        ),
        (
            "a long challenge",
            request_map(1, 2, None, None, Some(vec![0x0c; 17]), sig()),
        ),
    ] {
        assert!(
            RefreshRequest::decode(&bytes).is_err(),
            "{name} is accepted"
        );
    }
}

#[test]
fn s9e_refresh_a_hostile_request_is_refused() {
    let (_, original) = valid_requests().remove(0);
    let bytes = original.encode().expect("encodes");
    // Every truncation and every extension.
    for len in 0..bytes.len() {
        assert!(
            RefreshRequest::decode(&bytes[..len]).is_err(),
            "cut at {len}"
        );
    }
    let mut longer = bytes.to_vec();
    longer.push(0);
    assert!(RefreshRequest::decode(&longer).is_err());
    // Every single byte flip is refused or decodes to something that is not the original (a flipped
    // byte of the account decodes, to another account).
    for position in 0..bytes.len() {
        let mut flipped = bytes.to_vec();
        flipped[position] ^= 0x01;
        if let Ok(decoded) = RefreshRequest::decode(&flipped) {
            assert!(
                !same_request(&decoded, &original),
                "byte {position}: a changed request decodes to the original"
            );
        }
    }
    // Client versions: two parts, a part above u16, a text.
    for (name, version) in [
        (
            "two parts",
            Value::Array(vec![Value::Uint(1), Value::Uint(2)]),
        ),
        (
            "a part above u16",
            Value::Array(vec![Value::Uint(1), Value::Uint(2), Value::Uint(65_536)]),
        ),
        ("a text", Value::Text("1.2.3".to_string())),
    ] {
        let map = IntMapBuilder::new()
            .put(1, Value::Bytes(account().as_bytes().to_vec()))
            .put(2, Value::Uint(1))
            .put(3, Value::Bytes(device().as_bytes().to_vec()))
            .put(4, Value::Uint(1))
            .put(5, Value::Bytes(vec![0x44; 16]))
            .put(6, Value::Bytes(vec![0x07; 32]))
            .put(9, version)
            .build()
            .expect("a map");
        let bytes = encode(&map, &BIG).expect("encodes");
        assert!(RefreshRequest::decode(&bytes).is_err(), "{name}");
    }
    // An unknown key is ignored, a key `crit` lists and nobody knows is refused.
    let base = || {
        IntMapBuilder::new()
            .put(1, Value::Bytes(account().as_bytes().to_vec()))
            .put(2, Value::Uint(1))
            .put(3, Value::Bytes(device().as_bytes().to_vec()))
            .put(4, Value::Uint(1))
            .put(5, Value::Bytes(vec![0x44; 16]))
            .put(6, Value::Bytes(vec![0x07; 32]))
            .put(
                9,
                Value::Array(vec![Value::Uint(1), Value::Uint(2), Value::Uint(3)]),
            )
    };
    let ignored = encode(
        &base().put(30, Value::Uint(1)).build().expect("a map"),
        &BIG,
    )
    .expect("bytes");
    assert!(RefreshRequest::decode(&ignored).is_ok());
    let required = encode(
        &base()
            .put(30, Value::Uint(1))
            .crit(&[30])
            .build()
            .expect("a map"),
        &BIG,
    )
    .expect("bytes");
    assert!(RefreshRequest::decode(&required).is_err());
}

#[test]
fn s9e_refresh_a_token_never_prints() {
    let r = request(
        RefreshSubject::Device(device()),
        RefreshProof::Token(token()),
    );
    let text = format!("{r:?} {:?} {:?}", r.proof, token());
    assert!(text.contains("***"), "{text}");
    assert!(!text.contains("0707"), "the secret's bytes in {text}");
    assert!(!text.contains("07, 07"), "the secret's bytes in {text}");
    let whole_family = "44".repeat(16);
    assert!(!text.contains(&whole_family), "a whole family id in {text}");
    let key = request(
        RefreshSubject::Device(device()),
        RefreshProof::Key(key_proof()),
    );
    let text = format!("{key:?}");
    assert!(
        !text.contains(&"5a".repeat(16)),
        "a whole signature in {text}"
    );
    // The answer too.
    let answer = RefreshAnswerWire::granted(vec![1, 2, 3], Some(token()), None, Vec::new());
    let text = format!("{answer:?}");
    assert!(!text.contains("0707") && text.contains("***"), "{text}");
}

fn real_credential() -> (Fixture, Vec<u8>) {
    let (f, credential) = issue_fixture();
    let bytes = credential.encode().expect("encodes");
    (f, bytes)
}

fn word(w: &str) -> SmallText {
    SmallText::new(w).expect("a word")
}

fn same_answer(a: &RefreshAnswerWire, b: &RefreshAnswerWire) -> bool {
    let tokens = match (&a.token, &b.token) {
        (None, None) => true,
        (Some(x), Some(y)) => x.family() == y.family() && x.secret().ct_eq(y.secret()),
        _ => false,
    };
    tokens
        && a.credential == b.credential
        && a.family_expires_at == b.family_expires_at
        && a.notices == b.notices
        && a.reason == b.reason
        && a.spent_nothing == b.spent_nothing
        && a.hold_until == b.hold_until
        && a.min_client == b.min_client
}

fn day(n: u64) -> DayStart {
    DayStart::from_start(at(n * 86_400)).expect("a day start")
}

fn valid_answers() -> Vec<(&'static str, RefreshAnswerWire)> {
    let (_, credential) = real_credential();
    vec![
        (
            "granted_device",
            RefreshAnswerWire::granted(
                credential.clone(),
                Some(token()),
                Some(day(20_090)),
                vec![word("sign_in_soon")],
            ),
        ),
        (
            "granted_app",
            RefreshAnswerWire::granted(credential, None, None, Vec::new()),
        ),
        (
            "refused_token_reuse",
            RefreshAnswerWire::refused(RefusalWord::TokenReuse, false, None, None),
        ),
        (
            "refused_pending",
            RefreshAnswerWire::refused(
                RefusalWord::DevicePending,
                true,
                Some(at(1_900_000_000)),
                None,
            ),
        ),
        (
            "refused_too_old",
            RefreshAnswerWire::refused(
                RefusalWord::ClientTooOld,
                true,
                None,
                Some(ClientVersion::new(2, 0, 70_u16)),
            ),
        ),
    ]
}

#[test]
fn s9e_refresh_an_answer_round_trips_byte_for_byte() {
    for (name, original) in valid_answers() {
        let bytes = original.encode().expect("encodes");
        let decoded = RefreshAnswerWire::decode(&bytes).expect("decodes");
        assert!(same_answer(&original, &decoded), "{name}");
        assert_eq!(
            decoded.encode().expect("again").as_slice(),
            bytes.as_slice(),
            "{name}"
        );
    }
    for reason in RefusalWord::ALL {
        let answer = RefreshAnswerWire::refused(*reason, false, None, None);
        let bytes = answer.encode().expect("encodes");
        let decoded = RefreshAnswerWire::decode(&bytes).expect("decodes");
        assert_eq!(decoded.reason, Some(*reason));
    }
}

#[test]
fn s9e_refresh_the_credential_in_an_answer_verifies() {
    let (f, credential_bytes) = real_credential();
    let answer = RefreshAnswerWire::granted(credential_bytes, Some(token()), None, Vec::new());
    let bytes = answer.encode().expect("encodes");
    let decoded = RefreshAnswerWire::decode(&bytes).expect("decodes");
    let inner = decoded.credential.expect("a credential");
    assert!(Credential::decode(&inner).is_ok());
    assert!(
        verify_credential_bytes(&inner, &f.anchors, &placement(), now(ISSUED), &f.rules).is_ok()
    );
}

#[test]
fn s9e_refresh_an_answer_is_a_grant_or_a_refusal_and_nothing_else() {
    let cred = || Some(vec![9u8; 40]);
    let mk = |credential: Option<Vec<u8>>, reason: Option<RefusalWord>| RefreshAnswerWire {
        credential,
        token: None,
        family_expires_at: None,
        notices: Vec::new(),
        reason,
        spent_nothing: false,
        hold_until: None,
        min_client: None,
    };
    let refused = |a: RefreshAnswerWire| a.encode().err() == Some(CanonicalError::NotCanonical);
    assert!(mk(cred(), None).encode().is_ok());
    assert!(mk(None, Some(RefusalWord::Unauthorized)).encode().is_ok());
    assert!(refused(mk(cred(), Some(RefusalWord::Unauthorized))), "both");
    assert!(refused(mk(None, None)), "neither");
    assert!(refused(mk(Some(Vec::new()), None)), "an empty credential");
    assert!(
        refused(mk(Some(vec![0; CREDENTIAL_MAX + 1]), None)),
        "a credential above the limit"
    );
    assert!(mk(Some(vec![0; CREDENTIAL_MAX]), None).encode().is_ok());
    let mut a = mk(None, Some(RefusalWord::Unauthorized));
    a.token = Some(token());
    assert!(refused(a), "a token on a refusal");
    let mut a = mk(None, Some(RefusalWord::Unauthorized));
    a.family_expires_at = Some(day(1));
    assert!(refused(a), "a family expiry on a refusal");
    let mut a = mk(None, Some(RefusalWord::Unauthorized));
    a.notices = vec![word("x")];
    assert!(refused(a), "notices on a refusal");
    let mut a = mk(cred(), None);
    a.spent_nothing = true;
    assert!(refused(a), "spent_nothing on a grant");
    let mut a = mk(cred(), None);
    a.hold_until = Some(at(5));
    assert!(refused(a), "a hold on a grant");
    let mut a = mk(cred(), None);
    a.min_client = Some(ClientVersion::new(1, 0, 0));
    assert!(refused(a), "a minimum version on a grant");
    let mut a = mk(cred(), None);
    a.notices = (0..=MAX_NOTICES).map(|i| word(&format!("n{i}"))).collect();
    assert!(refused(a), "too many notices");
    let mut a = mk(cred(), None);
    a.notices = vec![SmallText::new("Not A Word").expect("printable")];
    assert!(refused(a), "a notice that is no machine word");
    let mut a = mk(cred(), None);
    a.notices = (0..MAX_NOTICES).map(|i| word(&format!("n{i}"))).collect();
    assert!(a.encode().is_ok(), "the most notices");
}

fn answer_map(entries: Vec<(u64, Value)>) -> Vec<u8> {
    let mut b = IntMapBuilder::new();
    for (key, value) in entries {
        b = b.put(key, value);
    }
    encode(&b.build().expect("a map"), &BIG).expect("encodes")
}

#[test]
fn s9e_refresh_a_hostile_answer_is_refused() {
    let cred = || (1u64, Value::Bytes(vec![9; 40]));
    let reason = |w: &str| (6u64, Value::Text(w.to_string()));
    // The controls.
    assert!(RefreshAnswerWire::decode(&answer_map(vec![cred()])).is_ok());
    assert!(RefreshAnswerWire::decode(&answer_map(vec![reason("unauthorized")])).is_ok());
    for (name, bytes, expected) in [
        (
            "both",
            answer_map(vec![cred(), reason("unauthorized")]),
            WireError::Malformed,
        ),
        (
            "neither",
            answer_map(vec![(4, Value::Uint(0))]),
            WireError::Malformed,
        ),
        (
            "an unknown reason word",
            answer_map(vec![reason("past_due")]),
            WireError::UnknownWord,
        ),
        (
            "spent_nothing written as false",
            answer_map(vec![reason("unauthorized"), (7, Value::Bool(false))]),
            WireError::NotCanonical,
        ),
        (
            "a family without a secret",
            answer_map(vec![cred(), (2, Value::Bytes(vec![1; 16]))]),
            WireError::Malformed,
        ),
        (
            "a secret without a family",
            answer_map(vec![cred(), (3, Value::Bytes(vec![1; 32]))]),
            WireError::Malformed,
        ),
        (
            "a family expiry that is not a day start",
            answer_map(vec![cred(), (4, Value::Uint(86_400 * 5 + 1))]),
            WireError::OutOfRange(4),
        ),
        (
            "five notices",
            answer_map(vec![
                cred(),
                (
                    5,
                    Value::Array((0..5).map(|i| Value::Text(format!("n{i}"))).collect()),
                ),
            ]),
            WireError::TooMany,
        ),
        (
            "an empty notices array",
            answer_map(vec![cred(), (5, Value::Array(Vec::new()))]),
            WireError::NotCanonical,
        ),
        (
            "a notice that is a number",
            answer_map(vec![cred(), (5, Value::Array(vec![Value::Uint(1)]))]),
            WireError::Malformed,
        ),
        (
            "a notice that is no machine word",
            answer_map(vec![
                cred(),
                (5, Value::Array(vec![Value::Text("Not A Word".to_string())])),
            ]),
            WireError::Malformed,
        ),
        (
            "a spent flag on a grant",
            answer_map(vec![cred(), (7, Value::Bool(true))]),
            WireError::Malformed,
        ),
    ] {
        assert_eq!(
            RefreshAnswerWire::decode(&bytes).err(),
            Some(expected),
            "{name}"
        );
    }
    // Every truncation of a real answer is refused.
    let (_, original) = valid_answers().remove(0);
    let bytes = original.encode().expect("encodes");
    for len in 0..bytes.len() {
        assert!(
            RefreshAnswerWire::decode(&bytes[..len]).is_err(),
            "cut at {len}"
        );
    }
}

fn request_rows() -> Vec<Row> {
    valid_requests()
        .into_iter()
        .map(|(name, r)| {
            let bytes = r.encode().expect("encodes");
            Row::new(
                &format!("refresh_request_{name}"),
                "refresh_request",
                "",
                &to_hex(&bytes),
                "token secret 07 x32, family 44 x16, challenge 0c x16, signature 5a x64, client 3.14.159",
                "ok",
            )
        })
        .collect()
}

fn answer_rows() -> Vec<Row> {
    valid_answers()
        .into_iter()
        .map(|(name, a)| {
            let bytes = a.encode().expect("encodes");
            Row::new(
                &format!("refresh_answer_{name}"),
                "refresh_answer",
                "",
                &to_hex(&bytes),
                "credential: the fixture's credential_device_wire where present",
                "ok",
            )
        })
        .collect()
}

#[test]
fn s9e_14_the_refresh_request_vectors_are_what_the_crate_computes() {
    check_or_update("refresh_request.json", &request_rows());
}

#[test]
fn s9e_14_the_refresh_answer_vectors_are_what_the_crate_computes() {
    check_or_update("refresh_answer.json", &answer_rows());
}

#[test]
fn s9e_14_the_refresh_vector_files_decode_without_the_code_that_wrote_them() {
    for file in ["refresh_request.json", "refresh_answer.json"] {
        let text = std::fs::read_to_string(vector_path(file)).unwrap_or_else(|e| {
            panic!("{file} is missing ({e}): UPDATE_VECTORS=1 writes it, never in CI")
        });
        let rows = parse_json(&text);
        assert!(!rows.is_empty());
        for row in rows {
            let bytes = from_hex(&row.hex);
            match row.kind.as_str() {
                "refresh_request" => {
                    let r = RefreshRequest::decode(&bytes).expect("a request");
                    assert_eq!(
                        r.encode().expect("again").as_slice(),
                        bytes.as_slice(),
                        "{}",
                        row.name
                    );
                }
                "refresh_answer" => {
                    let a = RefreshAnswerWire::decode(&bytes).expect("an answer");
                    assert_eq!(
                        a.encode().expect("again").as_slice(),
                        bytes.as_slice(),
                        "{}",
                        row.name
                    );
                }
                other => panic!("{}: an unknown kind {other}", row.name),
            }
        }
    }
}
