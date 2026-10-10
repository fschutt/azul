//! A paid checkout's period tokens (AZLINSEC17 F24, RFC 9474 RSABSSA-SHA384-PSS-Randomized):
//! blinded here, blind-signed by the token server against the issue key, finalized and checked
//! here, kept per drive until each buys the drive a month.

use std::sync::Arc;

use azul_storage::{testing::TempDir, Method};
use base64::{engine::general_purpose::STANDARD, Engine};
use rsa::{
    pkcs8::{EncodePublicKey, LineEnding},
    pss::{Signature, VerifyingKey},
    signature::Verifier,
    BigUint, RsaPublicKey,
};
use sha2::Sha384;

use super::{header, json, Fake, Shared, TOKEN};
use crate::{
    bundle::PeriodTokens,
    period::{issue_tokens, token_message, Issuer, PeriodToken, PeriodTokenStore},
    token::{TokenError, TokenServer},
};

/// A 2048-bit test issuer key (made for these tests with openssl; e = 65537): its modulus.
const N1: &str = concat!(
    "a9c7555148d74c3decb0475a2f078a3a1c7c2ce8b864ce41c2cae2cd0e15a7c8",
    "e599342edfa40a995d42101b9c47c0bb91b89085d8c831e55127c7a4f621f266",
    "f838a3cfc5f19d939867bed503ca2ce4a4fc0e765d4f21f9c78e2dadbfddee48",
    "b117ead2db705a081e48c15ae6e10c1d93e1c32859164533ac86c5b19256050f",
    "7bd457e6dad74d1d36e1129c4750b4ca436ba6f011fa10e0080dae3ffdb832e5",
    "cb9b5f83c388fce874bc101e8fd42d67041c57ea2fd8577c300eb4efa40b2ed8",
    "8d9614edefaaba3a5380024954d1964c1541a2b7fe6364aea35203d2555d2b8a",
    "2b17e0ec41cf86d4c574e7c083f9856c5a8b703393c39735055c85fd44b726b9",
);
/// Its private exponent: what the token server's blind signature raises a blinded message to.
const D1: &str = concat!(
    "105b5e02e8cba552c9fce9c2ec89036e39d454d74efc974a8aa3d55a002361f8",
    "def5f5ab166ccd809d15824bc6b0bb06d7313aeb4a496f55328e6c939e0b0339",
    "9c6c888bb9fc5f3c1b10d3b7de179a5fa4ed8bcf278a3a31c0621850870db0a9",
    "5c367baf38e9082384176a5981a8e8ebe89575a1eb8353c378b9bb4e32550b43",
    "9c749d331d341d8c750d4267399ad65208615156ed743c7d9128cdcd18c2bae0",
    "bbd6269d52899b8af767af241d428e84671485b569c6b41de4fd5f288a96b927",
    "144704392226842f70c44588566c7921f53e1237c802d964588f35efd04c2230",
    "309276cd67b63dec8f962e10be1b4329d9a33626ecb72586972478b4300f09a1",
);
/// Another test issuer key (another tier's, another year's).
const N2: &str = concat!(
    "bee59c1b9fe8a18ebda65ca50ff01bb20ac947339a7e6e4853c1973e97cbb373",
    "fa896abacc71ac468a481d65677f987b8b53167f398abd6d1f28b02c000dfcc2",
    "5ef53c353b13675ba5a55d9856e174b2a6529ed1fbb60ca091d5d6345da8661e",
    "0016190f3d2263ac1dac92ca36d3da3514c8a37e6966139690c5e8bfe559ba8b",
    "96e34f0dc3a417605f951ef885ebb52b138cba32e868cad077f746887252b6f1",
    "ad9c74a617cbab2af1da8b3e456ed1f300d0ac705bc887b95d81b82a7df2f63b",
    "ff34174f266ac227f53e01e065f0350e7e994fe83b4f29bf214c986d812a7144",
    "27a884db21eeceab4c9a8d9fff0ba1b79b67165b69f2b8012960f128475b27ff",
);
const D2: &str = concat!(
    "01472dec9cc17e64347d71249c9001444362b0b993e2fb23f8170ccfbd4d4b76",
    "0ad18709c4415eb4aa5423df0af6502954eb18304c10e47dd6809988d04769d3",
    "e61c62ce3d5c9431efb711cf72306517fbe6593d4230ec87739c19a89939ecb2",
    "b28540275dcc3f3e40518aa0946bb969c4b29ec887c71daca826bdb7e03eb71c",
    "4c932f3f5177094af19dfb4979320063f409b66153106f27bb4d5504c2723eec",
    "849a4f06e3152641384ffb05087d52d02f909f307842f55ccc464c98b3a565c0",
    "dc34a5999f6018dc0f67020f4166ad7b94602094804a5635c3a6d1a05931d922",
    "ededb905d0f0012e5262a9b858e87e335e2c5a74faec3e990288a96e379af0ad",
);

/// The issue key of the sealed sign-up of `ck_1`.
const ISSUE_KEY: &str = "Zm9yIHRoZSBwZXJpb2QgdG9rZW5zIG9mIGNrXzEgb25seQ";

fn big(hex: &str) -> BigUint {
    BigUint::parse_bytes(hex.as_bytes(), 16).unwrap()
}

fn public_key(n: &str) -> RsaPublicKey {
    RsaPublicKey::new(big(n), BigUint::from(65_537_u32)).unwrap()
}

/// The issuer's public key as the token server hands it out: SPKI PEM.
fn pem(n: &str) -> String {
    public_key(n).to_public_key_pem(LineEnding::LF).unwrap()
}

/// What the token server's `blind_sign` does: the blinded message raised to the private
/// exponent, as many bytes as the modulus, standard base64.
fn blind_sign(n: &str, d: &str, blinded: &str) -> String {
    let z = BigUint::from_bytes_be(&STANDARD.decode(blinded).unwrap());
    let s = z.modpow(&big(d), &big(n)).to_bytes_be();
    let mut out = vec![0_u8; 256 - s.len()];
    out.extend_from_slice(&s);
    STANDARD.encode(out)
}

#[test]
fn the_token_message_names_the_tier_the_year_and_the_nonce() {
    let nonce = "ab".repeat(32);
    assert_eq!(
        token_message("100GB", 2026, &nonce),
        format!("azlin-period-v1:100GB:2026:{nonce}")
    );
}

#[test]
fn a_blinded_period_token_signed_by_the_issuer_finalizes_into_an_rsassa_pss_signature_of_its_message(
) {
    let issuer = Issuer::new("100GB", 2026, &pem(N1)).unwrap();
    assert_eq!(issuer.key_id(), "100GB/2026");
    let blinded = issuer.blind().unwrap();
    let raw = STANDARD.decode(blinded.message()).unwrap();
    assert_eq!(raw.len(), 256, "as many bytes as the modulus");
    assert!(BigUint::from_bytes_be(&raw) < big(N1));
    let token = issuer
        .finalize(&blinded, &blind_sign(N1, D1, blinded.message()))
        .unwrap();
    assert_eq!((token.tier.as_str(), token.year), ("100GB", 2026));
    assert_eq!(token.nonce.len(), 64, "32 random bytes, hex");
    assert!(token
        .nonce
        .bytes()
        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)));
    assert_eq!(token.message(), token_message("100GB", 2026, &token.nonce));
    // RFC 9474's randomized variant: RSASSA-PSS (SHA-384, MGF1-SHA-384, a 48-byte salt) over a
    // 32-byte randomizer and the message - checked by the rsa crate's own verifier.
    let randomizer = STANDARD.decode(&token.randomizer).unwrap();
    assert_eq!(randomizer.len(), 32);
    let mut prepared = randomizer;
    prepared.extend_from_slice(token.message().as_bytes());
    let signature = STANDARD.decode(&token.signature).unwrap();
    assert_eq!(signature.len(), 256);
    let signature = Signature::try_from(signature.as_slice()).unwrap();
    VerifyingKey::<Sha384>::new(public_key(N1))
        .verify(&prepared, &signature)
        .unwrap();
    issuer.verify(&token).unwrap();
    // Not for another nonce, another tier, another year.
    let mut other = token.clone();
    other.nonce = "00".repeat(32);
    assert!(issuer.verify(&other).is_err());
    let mut other = token.clone();
    other.tier = String::from("1TB");
    assert!(issuer.verify(&other).is_err());
    assert!(Issuer::new("100GB", 2027, &pem(N1))
        .unwrap()
        .verify(&token)
        .is_err());
    // Two blindings share nothing.
    let second = issuer.blind().unwrap();
    assert_ne!(second.message(), blinded.message());
}

#[test]
fn a_blind_signature_by_another_key_or_of_another_size_finalizes_into_nothing() {
    let issuer = Issuer::new("100GB", 2026, &pem(N1)).unwrap();
    let blinded = issuer.blind().unwrap();
    assert!(matches!(
        issuer.finalize(&blinded, &blind_sign(N2, D2, blinded.message())),
        Err(TokenError::Protocol(_))
    ));
    assert!(matches!(
        issuer.finalize(&blinded, "c2ln"),
        Err(TokenError::Protocol(_))
    ));
    assert!(matches!(
        issuer.finalize(&blinded, "not base64!"),
        Err(TokenError::Protocol(_))
    ));
}

#[test]
fn an_issuer_key_that_is_no_rsa_public_key_is_refused() {
    for pem in ["", "-----BEGIN PUBLIC KEY-----\nAAAA\n-----END PUBLIC KEY-----\n"] {
        assert!(matches!(
            Issuer::new("100GB", 2026, pem),
            Err(TokenError::Protocol(_))
        ));
    }
    assert!(Issuer::new(" ", 2026, &pem(N1)).is_err(), "a tier");
}

/// A token server that hands out `keys` and blind-signs every blinded message with `n`/`d`,
/// answering `key_id` as the key it signed with.
fn issuing_server(
    keys: String,
    n: &'static str,
    d: &'static str,
    key_id: &'static str,
) -> Arc<Fake> {
    Fake::new(move |call, before| {
        if before == 0 {
            assert_eq!(call.method, Method::Get);
            assert_eq!(call.url, format!("{TOKEN}/v1/tokens/keys"));
            return Ok(json(200, &keys));
        }
        assert_eq!(call.method, Method::Post);
        assert_eq!(call.url, format!("{TOKEN}/v1/tokens/issue"));
        assert_eq!(header(call, "authorization"), None, "no drive token");
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        assert_eq!(body["checkout_id"], "ck_1");
        assert_eq!(body["issue_key"], ISSUE_KEY);
        let signatures: Vec<String> = body["blinded"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| blind_sign(n, d, b.as_str().unwrap()))
            .collect();
        let answer = serde_json::json!({
            "tier": "100GB", "key_id": key_id, "public_key_pem": pem(n),
            "blind_signatures": signatures,
        });
        Ok(json(200, &answer.to_string()))
    })
}

fn keys() -> String {
    serde_json::json!({"keys": [
        {"tier": "1TB", "year": 2026, "key_id": "1TB/2026", "public_key_pem": pem(N2)},
        {"tier": "100GB", "year": 2026, "key_id": "100GB/2026", "public_key_pem": pem(N1)},
    ]})
    .to_string()
}

fn grant(months: u32) -> PeriodTokens {
    PeriodTokens {
        checkout_id: String::from("ck_1"),
        months,
        issue_key: String::from(ISSUE_KEY),
    }
}

#[test]
fn a_paid_checkouts_months_of_period_tokens_are_blinded_issued_against_its_issue_key_and_finalized(
) {
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let tokens = issue_tokens(&server, &grant(3), "100GB").unwrap();
    assert_eq!(tokens.len(), 3, "one per paid month");
    let issuer = Issuer::new("100GB", 2026, &pem(N1)).unwrap();
    for token in &tokens {
        issuer.verify(token).unwrap();
    }
    assert_ne!(tokens[0].nonce, tokens[1].nonce);
    assert_ne!(tokens[1].nonce, tokens[2].nonce);
    let calls = fake.calls();
    assert_eq!(calls.len(), 2, "the keys, then one issue");
    let body: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
    assert_eq!(body["blinded"].as_array().unwrap().len(), 3);
}

#[test]
fn period_tokens_signed_by_another_key_than_the_one_they_were_blinded_for_are_refused() {
    // The token server signs with the next year's key (its year turned between the two calls).
    let fake = issuing_server(keys(), N2, D2, "100GB/2027");
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_tokens(&server, &grant(1), "100GB"),
        Err(TokenError::Protocol(_))
    ));
    // No key for the tier: nothing is issued.
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_tokens(&server, &grant(1), "10TB"),
        Err(TokenError::Protocol(_))
    ));
    assert_eq!(fake.calls().len(), 1, "only the keys were asked for");
    // No months, more than one call takes: nothing is sent.
    for months in [0, 25] {
        assert!(matches!(
            issue_tokens(&server, &grant(months), "100GB"),
            Err(TokenError::Config(_))
        ));
    }
    assert_eq!(fake.calls().len(), 1);
}

fn stored(nonce: &str) -> PeriodToken {
    PeriodToken {
        tier: String::from("100GB"),
        year: 2026,
        nonce: nonce.repeat(32),
        signature: String::from("c2lnbmF0dXJl"),
        randomizer: String::from("cmFuZG9taXplcg=="),
    }
}

#[test]
fn period_tokens_wait_in_a_file_of_their_drive_that_only_this_user_reads() {
    let dir = TempDir::new("azcloud-period");
    let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
    assert!(store.tokens("d_1").unwrap().is_empty());
    store.add("d_1", &[stored("aa"), stored("bb")]).unwrap();
    store.add("d_1", &[stored("bb")]).unwrap();
    store.add("d_2", &[stored("cc")]).unwrap();
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("aa"), stored("bb")]);
    assert!(store.remove("d_1", &stored("aa").nonce).unwrap());
    assert!(!store.remove("d_1", &stored("aa").nonce).unwrap());
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("bb")]);
    assert_eq!(store.tokens("d_2").unwrap(), vec![stored("cc")]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let path = store.path_of("d_1").unwrap();
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o077, 0, "{mode:o}");
    }
    assert!(store.add("../d_1", &[stored("aa")]).is_err());
    let shown = format!("{:?}", stored("aa"));
    assert!(!shown.contains("c2lnbmF0dXJl"), "Debug shows no signature: {shown}");
}

#[test]
fn a_period_token_buys_its_drive_a_month_with_the_drive_token_and_only_once() {
    let fake = Fake::new(|call, before| {
        Ok(if before == 0 {
            assert_eq!(call.method, Method::Post);
            assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/redeem"));
            assert_eq!(header(call, "authorization"), Some("Bearer dt_a.1.x"));
            let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
            assert_eq!(body["tier"], "100GB");
            assert_eq!(body["year"], 2026);
            assert_eq!(body["nonce"], "aa".repeat(32));
            assert_eq!(body["signature"], "c2lnbmF0dXJl");
            assert_eq!(body["randomizer"], "cmFuZG9taXplcg==");
            json(200, r#"{"period_until": "2026-12-07T00:00:00Z"}"#)
        } else {
            json(
                409,
                r#"{"error": "token_used", "message": "this token was already redeemed"}"#,
            )
        })
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let until = server
        .redeem_period_token("d_1", "dt_a.1.x", &stored("aa"))
        .unwrap();
    assert_eq!(until, azul_storage::time::parse_iso8601("2026-12-07T00:00:00Z"));
    match server.redeem_period_token("d_1", "dt_a.1.x", &stored("aa")) {
        Err(TokenError::Refused { status: 409, code, .. }) => assert_eq!(code, "token_used"),
        other => panic!("not refused as used: {other:?}"),
    }
}
