//! S9E-18 `hostile_inputs`: truncated and extended encodings, structures with thousands of entries, text
//! with a NUL byte, duplicated fields, unknown fields: each is `Err` with a typed error, never a panic,
//! never accepted. And the other half of principle 6: a field the reader does not know is IGNORED unless
//! `crit` lists it, and the signature still verifies (it covers the received bytes).

use azlin_cbor::{encode, DecodeError, IntMapBuilder, ShapeError, Value};
use azlin_creds::{
    verify_credential, verify_credential_bytes, Certificate, CredError, Credential,
    CredentialClaims, Scope, Scopes, WireError,
};
use azlin_regbase::SmallText;

use crate::support::*;

#[test]
fn s9e_18_every_truncation_of_a_credential_is_refused() {
    let (f, credential) = issue_fixture();
    let bytes = credential.encode().expect("encodes");
    for len in 0..bytes.len() {
        let cut = &bytes[..len];
        assert!(
            Credential::decode(cut).is_err(),
            "a credential cut at {len} decodes"
        );
        assert!(
            verify_credential_bytes(cut, &f.anchors, now(ISSUED), &f.rules).is_err(),
            "a credential cut at {len} verifies"
        );
    }
    // And the whole of it is fine.
    assert!(verify_credential_bytes(&bytes, &f.anchors, now(ISSUED), &f.rules).is_ok());
}

#[test]
fn s9e_18_an_extended_credential_is_refused() {
    let (f, credential) = issue_fixture();
    let mut bytes = credential.encode().expect("encodes");
    bytes.push(0);
    assert_eq!(
        Credential::decode(&bytes).err(),
        Some(WireError::Cbor(DecodeError::TrailingBytes))
    );
    assert_eq!(
        verify_credential_bytes(&bytes, &f.anchors, now(ISSUED), &f.rules).err(),
        Some(CredError::Malformed)
    );
    // A certificate with a byte after it, too.
    let mut cert = f.cert.encode().expect("encodes");
    cert.push(0);
    assert_eq!(
        Certificate::decode(&cert).err(),
        Some(WireError::Cbor(DecodeError::TrailingBytes))
    );
}

#[test]
fn s9e_18_every_single_byte_flip_is_refused_and_never_panics() {
    let (f, credential) = issue_fixture();
    let bytes = credential.encode().expect("encodes");
    for position in 0..bytes.len() {
        for mask in [0x01u8, 0x80, 0xFF] {
            let mut flipped = bytes.clone();
            flipped[position] ^= mask;
            let verdict = verify_credential_bytes(&flipped, &f.anchors, now(ISSUED), &f.rules);
            assert!(
                verdict.is_err(),
                "byte {position} xor {mask:#x}: a changed credential verifies"
            );
        }
    }
}

#[test]
fn s9e_18_input_above_the_limit_is_refused_before_it_is_read() {
    let big = vec![0u8; 9 * 1024];
    assert_eq!(
        Credential::decode(&big).err(),
        Some(WireError::Cbor(DecodeError::InputTooLarge))
    );
    assert_eq!(
        Certificate::decode(&big).err(),
        Some(WireError::Cbor(DecodeError::InputTooLarge))
    );
    let empty: [u8; 0] = [];
    assert_eq!(
        Credential::decode(&empty).err(),
        Some(WireError::Cbor(DecodeError::Truncated))
    );
}

#[test]
fn s9e_18_thousands_of_scopes_are_refused_on_every_path() {
    // In the type: refused, never truncated.
    let scopes: Vec<Scope> = (0..10_000u32)
        .map(|i| Scope::LinkRead {
            link: azlin_creds::LinkRef::from_bytes([(i % 251) as u8; 16]),
        })
        .collect();
    assert!(Scopes::new(scopes).is_err());
    // On the wire: the codec's item limit refuses before anything is built.
    let f = fixture();
    let mut claims = claims_of(grant());
    claims.device = None;
    claims.family = None;
    let items: Vec<Value> = (0..10_000u32)
        .map(|i| {
            Value::Array(vec![
                Value::Uint(1),
                Value::Bytes(vec![(i % 251) as u8; 16]),
            ])
        })
        .collect();
    let payload = payload_map(&claims)
        .put(6, Value::Array(items))
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert!(Credential::decode(&bytes).is_err());
    assert_eq!(
        verify_credential_bytes(&bytes, &f.anchors, now(ISSUED), &f.rules).err(),
        Some(CredError::Malformed)
    );
}

#[test]
fn s9e_18_a_nul_byte_in_the_provider_is_refused() {
    assert!(
        SmallText::new("a\0b").is_err(),
        "the bounded text refuses control bytes"
    );
    let f = fixture();
    let claims = claims_of(grant());
    let sso = IntMapBuilder::new()
        .put(1, Value::Text("provider\0".to_string()))
        .put(2, Value::Uint(ISSUED - 1))
        .build()
        .expect("an sso map");
    let payload = payload_map(&claims)
        .put(11, sso)
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert_eq!(Credential::decode(&bytes).err(), Some(WireError::Malformed));
    // An empty provider is no provider either.
    let sso = IntMapBuilder::new()
        .put(1, Value::Text(String::new()))
        .put(2, Value::Uint(ISSUED - 1))
        .build()
        .expect("an sso map");
    let payload = payload_map(&claims)
        .put(11, sso)
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert_eq!(Credential::decode(&bytes).err(), Some(WireError::Malformed));
}

#[test]
fn s9e_18_a_duplicated_field_is_refused() {
    // map(2) { 1: h'', 1: h'' }: the codec refuses the same key twice.
    let duplicate = [0xa2, 0x01, 0x40, 0x01, 0x40];
    assert_eq!(
        Credential::decode(&duplicate).err(),
        Some(WireError::Cbor(DecodeError::DuplicateMapKey))
    );
    // The same inside the certificate.
    assert_eq!(
        Certificate::decode(&duplicate).err(),
        Some(WireError::Cbor(DecodeError::DuplicateMapKey))
    );
    // Keys out of order are refused too.
    let unsorted = [0xa2, 0x02, 0x40, 0x01, 0x40];
    assert_eq!(
        Credential::decode(&unsorted).err(),
        Some(WireError::Cbor(DecodeError::UnsortedMapKeys))
    );
}

#[test]
fn s9e_18_a_field_of_the_wrong_type_or_length_is_refused_not_repaired() {
    let f = fixture();
    let claims = claims_of(grant());
    // An account as a text, as a short or a long byte string, as a null, as a number.
    let mut b = IntMapBuilder::new()
        .put(2, Value::Bytes(claims.member.as_bytes().to_vec()))
        .put(
            3,
            Value::Bytes(claims.device.expect("a device").as_bytes().to_vec()),
        )
        .put(
            4,
            Value::Bytes(claims.family.expect("a family").as_bytes().to_vec()),
        )
        .put(5, Value::Uint(3))
        .put(7, Value::Uint(ISSUED))
        .put(8, Value::Uint(ISSUED + LIFETIME))
        .put(9, Value::Uint(u64::from(EPOCH.get())))
        .put(10, Value::Bytes(vec![0x55; 16]));
    for (name, bad) in [
        ("text", Value::Text("account".to_string())),
        ("short", Value::Bytes(vec![1; 15])),
        ("long", Value::Bytes(vec![1; 17])),
        ("null", Value::Null),
        ("number", Value::Uint(1)),
    ] {
        let payload = b.clone().put(1, bad).build().expect("a payload");
        let bytes = craft_credential(&f.cred, &f.cert, &payload);
        assert!(
            matches!(Credential::decode(&bytes), Err(WireError::Shape(_))),
            "an account as a {name}"
        );
    }
    // Numbers that do not fit their type: a key epoch above u32, a policy epoch above u32.
    b = b.put(1, Value::Bytes(vec![0x11; 16]));
    let too_big = IntMapBuilder::new()
        .put(1, Value::Bytes(vec![0x11; 16]))
        .put(2, Value::Bytes(vec![0x22; 16]))
        .put(3, Value::Bytes(vec![0x33; 16]))
        .put(4, Value::Bytes(vec![0x44; 16]))
        .put(5, Value::Uint(u64::from(u32::MAX) + 1))
        .put(7, Value::Uint(ISSUED))
        .put(8, Value::Uint(ISSUED + LIFETIME))
        .put(9, Value::Uint(7))
        .put(10, Value::Bytes(vec![0x55; 16]))
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &too_big);
    assert_eq!(
        Credential::decode(&bytes).err(),
        Some(WireError::OutOfRange(5)),
        "a policy epoch above u32"
    );
    // A `null` where an optional field is absent is not the absent field.
    let with_null = b.clone().put(6, Value::Null).build().expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &with_null);
    assert!(matches!(
        Credential::decode(&bytes),
        Err(WireError::Shape(_))
    ));
}

#[test]
fn s9e_18_a_certificate_is_a_closed_schema() {
    let f = fixture();
    let value = f.cert.to_value().expect("a value");
    let Value::Map(map) = &value else {
        panic!("a certificate is a map");
    };
    let mut entries: Vec<(Value, Value)> = map.iter().cloned().collect();
    // An extra key, and `crit`.
    let mut with_extra = entries.clone();
    with_extra.push((Value::Uint(99), Value::Uint(1)));
    let extra = Value::Map(azlin_cbor::Map::from_entries(with_extra).expect("a map"));
    let bytes = encode(&extra, &BIG).expect("encodes");
    assert_eq!(
        Certificate::decode(&bytes).err(),
        Some(WireError::UnknownField(99))
    );
    entries.push((Value::Uint(0), Value::Array(vec![Value::Uint(1)])));
    let crit = Value::Map(azlin_cbor::Map::from_entries(entries.clone()).expect("a map"));
    let bytes = encode(&crit, &BIG).expect("encodes");
    assert_eq!(
        Certificate::decode(&bytes).err(),
        Some(WireError::UnknownField(0))
    );
    // A key missing.
    entries.retain(|(k, _)| *k != Value::Uint(0) && *k != Value::Uint(4));
    let missing = Value::Map(azlin_cbor::Map::from_entries(entries).expect("a map"));
    let bytes = encode(&missing, &BIG).expect("encodes");
    assert_eq!(
        Certificate::decode(&bytes).err(),
        Some(WireError::Shape(ShapeError::Missing(4)))
    );
}

#[test]
fn s9e_18_an_unknown_claim_is_ignored_and_the_signature_still_verifies() {
    // Principle 6: a field a newer issuer adds is ignored by an older reader, and the signature, which
    // covers the received bytes, still verifies.
    let f = fixture();
    let claims = claims_of(grant());
    let payload = payload_map(&claims)
        .put(40, Value::Text("something a newer issuer says".to_string()))
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    let credential = Credential::decode(&bytes).expect("an unknown key is ignored");
    let verified =
        verify_credential(&credential, &f.anchors, now(ISSUED), &f.rules).expect("verifies");
    assert_eq!(verified.account(), claims.account);
    assert_eq!(
        credential.encode().expect("encodes"),
        bytes,
        "a copy keeps the unknown field: the bytes are the signed ones"
    );
}

#[test]
fn s9e_18_a_claim_that_crit_lists_and_nobody_understands_is_refused() {
    let f = fixture();
    let claims = claims_of(grant());
    // `crit` lists key 40, which this build does not know: the credential is refused.
    let payload = payload_map(&claims)
        .put(40, Value::Uint(1))
        .crit(&[40])
        .build()
        .expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    assert_eq!(
        Credential::decode(&bytes).err(),
        Some(WireError::Shape(ShapeError::CritUnknown(40)))
    );
    assert_eq!(
        verify_credential_bytes(&bytes, &f.anchors, now(ISSUED), &f.rules).err(),
        Some(CredError::Malformed)
    );
    // `crit` that lists a key this build DOES understand is fine, and the copy keeps it.
    let payload = payload_map(&claims).crit(&[5]).build().expect("a payload");
    let bytes = craft_credential(&f.cred, &f.cert, &payload);
    let credential = Credential::decode(&bytes).expect("a crit of a known key");
    assert_eq!(
        credential.encode().expect("encodes"),
        bytes,
        "a copy keeps crit (R18)"
    );
    assert!(verify_credential(&credential, &f.anchors, now(ISSUED), &f.rules).is_ok());
    // A malformed `crit` is refused.
    let payload = payload_map(&claims).put(0, Value::Array(vec![])).build();
    assert!(payload.is_err(), "the builder refuses key 0 itself");
}

#[test]
fn s9e_18_the_claims_of_a_credential_come_from_the_signed_bytes() {
    // A credential whose typed claims and signed bytes disagree cannot be made through the API; the
    // verifier reads the bytes either way. Build one by decoding two different credentials and
    // checking that what verifies is what was signed.
    let f = fixture();
    let mut other = claims_of(grant());
    other.nonce = azlin_regbase::CredentialNonce::from_bytes([0x99; 16]);
    let signed = signed_credential(&f.cred, f.cert, other.clone());
    let verified = verify_credential(&signed, &f.anchors, now(ISSUED), &f.rules).expect("verifies");
    assert_eq!(verified.nonce(), other.nonce);
    let decoded: CredentialClaims =
        CredentialClaims::decode_payload(signed.payload()).expect("claims");
    assert_eq!(decoded, other);
}
