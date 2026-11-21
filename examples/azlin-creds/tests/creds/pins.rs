//! S9E-14 `the_signing_input_is_pinned`: the signing input of a fixed credential and of a fixed certificate
//! equal HEX LITERALS written here (the domain, a NUL, the canonical bytes: no length byte, no other
//! prefix), and the signatures of the fixture's keys over them equal literals too. The literals are the
//! wire form of `azlin-credential-v2` and `azlin-credential-cert-v1` as this crate writes them; INT-WIRE
//! compares them with S5's contract fixtures (the files `credential_v2.valid.cbor` and
//! `credential_cert_v1.valid.cbor` of the brief are the vectors of `tests/vectors/`, copied there).

use azlin_creds::{signing_preimage, Canonical, Signer, CREDENTIAL_CERT_V1, CREDENTIAL_V2};

use crate::support::*;

/// The canonical claims of `claims_of(grant())`: a map of nine entries, keys 1 to 5 and 7 to 10.
const CREDENTIAL_PAYLOAD_HEX: &str = "a90150111111111111111111111111111111110250222222222222222222222222222222220350333333333333333333333333333333330450444444444444444444444444444444440503071a000186a0081a000194b009070a5055555555555555555555555555555555";
/// The signature of credential seed 2 over `azlin-credential-v2 || 0x00 || payload`.
const CREDENTIAL_SIG_HEX: &str = "c784f2c3d0aa97f580fab4c633be792c66e83f45c43695291b40f9510a68e889f7f9d991103fd50a6e0e39450de7a6d1dc52331d5aed82bac3d08f2c396d6c0a";
/// The body of the fixture's certificate: a map of six entries, keys 1 to 6.
const CERT_BODY_HEX: &str = "a6010102070358208139770ea87d175f56a35466c34c7ecccb8d8a91b4ee37a25df60f5b8fc9b3940458208a88e3dd7409f195fd52db2d3cba5d72ca6709bf1d94121bf3748801b40f6f5c0519c350061a00285050";
/// The signature of identity seed 1 over `azlin-credential-cert-v1 || 0x00 || body`.
const CERT_SIG_HEX: &str = "390ab07e710a981393be390ba512999a504d786030715feb4406501aad4152abf3d0fa0e679bec32113dcaec74bd83f8a0261ec9853c399ddccad2d6a8ca980f";

#[test]
fn s9e_14_the_signing_input_of_a_fixed_credential_is_a_literal() {
    let claims = claims_of(grant());
    let payload = claims.canonical_bytes().expect("canonical bytes");
    assert_eq!(
        to_hex(&payload),
        CREDENTIAL_PAYLOAD_HEX,
        "the canonical claims"
    );
    let preimage = signing_preimage(&claims).expect("a preimage");
    let expected = [
        b"azlin-credential-v2\0".as_slice(),
        from_hex(CREDENTIAL_PAYLOAD_HEX).as_slice(),
    ]
    .concat();
    assert_eq!(preimage, expected, "domain || 0x00 || bytes");
    assert_eq!(
        &preimage[..20],
        b"azlin-credential-v2\0",
        "the prefix: the domain and its NUL"
    );
    assert_eq!(preimage[19], 0, "the NUL ends the 19 bytes of the domain");
    assert_eq!(preimage.len(), 20 + 107, "no length byte, no other prefix");
    let f = fixture();
    let sig = f.cred.sign(&CREDENTIAL_V2, &payload).expect("a signature");
    assert_eq!(
        to_hex(sig.as_bytes()),
        CREDENTIAL_SIG_HEX,
        "Ed25519 over the preimage"
    );
}

#[test]
fn s9e_14_the_signing_input_of_the_fixed_certificate_is_a_literal() {
    let f = fixture();
    let body = f.cert.canonical_bytes().expect("the body");
    assert_eq!(to_hex(&body), CERT_BODY_HEX, "the canonical body");
    let preimage = signing_preimage(&f.cert).expect("a preimage");
    let expected = [
        b"azlin-credential-cert-v1\0".as_slice(),
        from_hex(CERT_BODY_HEX).as_slice(),
    ]
    .concat();
    assert_eq!(preimage, expected, "domain || 0x00 || bytes");
    assert_eq!(preimage[24], 0, "the NUL ends the 24 bytes of the domain");
    assert_eq!(
        to_hex(f.cert.identity_sig.as_bytes()),
        CERT_SIG_HEX,
        "the certificate's signature"
    );
    let again = f
        .identity
        .sign(&CREDENTIAL_CERT_V1, &body)
        .expect("a signature");
    assert_eq!(
        to_hex(again.as_bytes()),
        CERT_SIG_HEX,
        "Ed25519 is deterministic"
    );
}
