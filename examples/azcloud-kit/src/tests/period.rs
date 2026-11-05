//! A paid checkout's period tokens (AZLINSEC17 F24, RFC 9474 RSABSSA-SHA384-PSS-Randomized):
//! blinded here, blind-signed by the token server against the issue key, finalized and checked
//! here, kept per drive until each buys the drive a month.

use std::{
    sync::{
        atomic::{AtomicU64, AtomicUsize, Ordering},
        Arc,
    },
    time::Duration,
};

use azlin_contract::IssueRefusal;
use azul_storage::{
    keyring::KeyringNamespace,
    config::keyring_key,
    keyring::{KeyringStore, MemoryKeyring},
    testing::TempDir,
    time::iso8601 as rfc3339,
    HttpReply, Method,
};
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
    lock::LockDir,
    period::{
        ended_words, issue_next, look_at_drive, redeem_due, token_message, Issued, Issuer,
        PeriodToken, PeriodTokenStore, Redeemed,
    },
    shared::SharedKeyring,
    token::{TokenError, TokenServer, RECOVERY_MEMBER},
    vault::{DriveTokens, VaultGrant, VaultState},
    AzlinSession,
};

/// A 2048-bit test issuer key (made for these tests with openssl; e = 65537): its modulus.
pub(crate) const N1: &str = concat!(
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
pub(crate) const D1: &str = concat!(
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
pub(crate) const ISSUE_KEY: &str = "Zm9yIHRoZSBwZXJpb2QgdG9rZW5zIG9mIGNrXzEgb25seQ";

fn big(hex: &str) -> BigUint {
    BigUint::parse_bytes(hex.as_bytes(), 16).unwrap()
}

fn public_key(n: &str) -> RsaPublicKey {
    RsaPublicKey::new(big(n), BigUint::from(65_537_u32)).unwrap()
}

/// The issuer's public key as the token server hands it out: SPKI PEM.
pub(crate) fn pem(n: &str) -> String {
    public_key(n).to_public_key_pem(LineEnding::LF).unwrap()
}

/// What the token server's `blind_sign` does: the blinded message raised to the private
/// exponent, as many bytes as the modulus, standard base64.
pub(crate) fn blind_sign(n: &str, d: &str, blinded: &str) -> String {
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

/// When the tests' checkout `ck_1` (3 months) was approved: a day before [`NOW`].
pub(crate) const APPROVED: u64 = NOW - DAY;
/// A month's instalment (the contract's `INSTALMENT_SECS`).
pub(crate) const MONTH: u64 = 30 * DAY;

/// A token server that hands out `keys` and blind-signs every blinded message with `n`/`d` for
/// the key `key_id` - the one the request must name -, counting the months of `ck_1` it issued.
pub(crate) fn issuing_server(
    keys: String,
    n: &'static str,
    d: &'static str,
    key_id: &'static str,
) -> Arc<Fake> {
    let issued = Arc::new(AtomicU64::new(0));
    Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            assert_eq!(call.method, Method::Get);
            return Ok(json(200, &keys));
        }
        Ok(signed(call, n, d, key_id, &issued))
    })
}

/// The token server's answer to an issue request `call` for `key_id`, signed with `n`/`d`, with
/// `ck_1`'s schedule after it (`issued` counts its months).
fn signed(call: &azul_storage::HttpCall, n: &str, d: &str, key_id: &str, issued: &AtomicU64) -> HttpReply {
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, format!("{TOKEN}/v1/tokens/issue"));
    assert_eq!(header(call, "authorization"), None, "no drive token");
    let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["checkout_id"], "ck_1");
    assert_eq!(body["issue_key"], ISSUE_KEY);
    assert_eq!(body["key_id"], key_id, "the key the messages were blinded for");
    let signatures: Vec<String> = body["blinded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| blind_sign(n, d, b.as_str().unwrap()))
        .collect();
    let after = issued.fetch_add(signatures.len() as u64, Ordering::SeqCst) + signatures.len() as u64;
    let mut schedule = serde_json::json!({"months": 3, "issued": after});
    if after < 3 {
        schedule["next_issue_at"] = serde_json::json!(rfc3339(APPROVED + after * MONTH));
    }
    let answer = serde_json::json!({
        "tier": "100GB", "key_id": key_id, "public_key_pem": pem(n),
        "blind_signatures": signatures, "schedule": schedule,
    });
    json(200, &answer.to_string())
}

pub(crate) fn keys() -> String {
    serde_json::json!({"keys": [
        {"tier": "1TB", "year": 2026, "key_id": "1TB/2026", "public_key_pem": pem(N2)},
        {"tier": "100GB", "year": 2026, "key_id": "100GB/2026", "public_key_pem": pem(N1)},
    ]})
    .to_string()
}

fn store_in(dir: &TempDir) -> PeriodTokenStore {
    PeriodTokenStore::new(dir.path().join("period-tokens"))
}

/// Drive `d_1`'s tokens on this computer, with `ck_1`'s grant of 3 months.
fn granted(dir: &TempDir) -> DriveTokens {
    let tokens = store_in(dir).on_this_device("d_1").unwrap();
    tokens.keep_grant(VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY)).unwrap();
    tokens
}

#[test]
fn a_paid_checkouts_first_month_is_issued_against_its_issue_key_and_kept_with_its_schedule() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let issued = match issue_next(&server, &tokens, "ck_1", NOW).unwrap() {
        Issued::Tokens(issued) => issued,
        other => panic!("not issued: {other:?}"),
    };
    assert_eq!(issued.len(), 1, "one month at a time");
    let issuer = Issuer::new("100GB", 2026, &pem(N1)).unwrap();
    issuer.verify(&issued[0]).unwrap();
    let calls = fake.calls();
    assert_eq!(calls.len(), 2, "the keys, then one issue");
    let body: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
    assert_eq!(body["blinded"].as_array().unwrap().len(), 1);
    // Kept for the drive with the grant's schedule; the request they were issued for is done.
    let state = tokens.state().unwrap();
    assert_eq!(state.tokens, issued);
    let grant = state.grant("ck_1").unwrap();
    assert_eq!((grant.issued, grant.next_issue_at), (1, Some(APPROVED + MONTH)));
    assert!(state.issues.is_empty());
    // The next month is not due: nothing is asked.
    assert_eq!(
        issue_next(&server, &tokens, "ck_1", NOW).unwrap(),
        Issued::NotDue {
            next_issue_at: Some(APPROVED + MONTH)
        }
    );
    assert_eq!(fake.calls().len(), 2);
}

#[test]
fn a_lost_answer_is_asked_for_again_with_the_identical_request_however_late() {
    // The token server answers the same request (key, messages, order) with the same
    // signatures, counted once, as long as its tokens can be redeemed - so the request is kept
    // before it is sent, with the drive.
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let keys = keys();
    let issued = AtomicU64::new(0);
    let fake = Fake::new(move |call, n| match n {
        0 => Ok(json(200, &keys)),
        1 => Err(String::from("connection reset")),
        _ => Ok(signed(call, N1, D1, "100GB/2026", &issued)),
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Err(TokenError::Connect(_))
    ));
    let state = tokens.state().unwrap();
    let kept = state.open_issues("ck_1");
    assert_eq!(kept.len(), 1, "the request, kept");
    assert_eq!((kept[0].checkout_id(), kept[0].key_id()), ("ck_1", "100GB/2026"));
    let shown = format!("{:?}", kept[0]);
    assert!(!shown.contains(&kept[0].messages()[0]), "Debug shows no blinding: {shown}");
    // A month later (the app was not started): the very same request.
    match issue_next(&server, &tokens, "ck_1", NOW + 31 * DAY).unwrap() {
        Issued::Tokens(issued) => assert_eq!(issued.len(), 1),
        other => panic!("not issued: {other:?}"),
    }
    let calls = fake.calls();
    assert_eq!(calls.len(), 3, "no new keys, no new blinding");
    assert_eq!(calls[1].body, calls[2].body, "the identical request");
    assert!(tokens.state().unwrap().issues.is_empty());
}

#[test]
fn a_key_that_changed_since_the_keys_were_read_is_blinded_for_again_before_anything_is_signed() {
    // The year turned between GET /v1/tokens/keys and the issue: 409 key_changed names the
    // current key (with it, when the token server holds it) and nothing is signed or counted.
    for with_key in [true, false] {
        let dir = TempDir::new("azcloud-period");
        let tokens = granted(&dir);
        let old = keys();
        let new = serde_json::json!({"keys": [
            {"tier": "100GB", "year": 2027, "key_id": "100GB/2027", "public_key_pem": pem(N2)},
        ]})
        .to_string();
        let reads = AtomicUsize::new(0);
        let issued = AtomicU64::new(0);
        let fake = Fake::new(move |call, _| {
            if call.url.ends_with("/v1/tokens/keys") {
                let first = reads.fetch_add(1, Ordering::SeqCst) == 0;
                return Ok(json(200, if first { &old } else { &new }));
            }
            let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
            if body["key_id"] == "100GB/2026" {
                let answer = serde_json::json!({
                    "error": "key_changed", "message": "blind the messages for this key",
                    "key_id": "100GB/2027", "public_key_pem": with_key.then(|| pem(N2)),
                });
                return Ok(json(409, &answer.to_string()));
            }
            Ok(signed(call, N2, D2, "100GB/2027", &issued))
        });
        let transport = Shared(fake.clone());
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        let issued = match issue_next(&server, &tokens, "ck_1", NOW).unwrap() {
            Issued::Tokens(issued) => issued,
            other => panic!("not issued: {other:?}"),
        };
        let issuer = Issuer::new("100GB", 2027, &pem(N2)).unwrap();
        assert!(issued.iter().all(|t| t.year == 2027 && issuer.verify(t).is_ok()));
        let calls = fake.calls();
        let reads = calls.iter().filter(|c| c.url.ends_with("/v1/tokens/keys")).count();
        assert_eq!(reads, if with_key { 1 } else { 2 }, "the keys again only without the key");
        let issues: Vec<_> = calls
            .iter()
            .filter(|c| c.url.ends_with("/v1/tokens/issue"))
            .collect();
        assert_eq!(issues.len(), 2);
        assert_ne!(issues[0].body, issues[1].body, "blinded anew for the new key");
        assert!(tokens.state().unwrap().issues.is_empty());
    }
}

#[test]
fn period_tokens_signed_by_another_key_than_the_one_asked_for_are_refused() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    // The answer names another key than the request did.
    let listed = keys();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        let answer = serde_json::json!({
            "tier": "100GB", "key_id": "100GB/2025", "public_key_pem": pem(N2),
            "blind_signatures": [blind_sign(N2, D2, &STANDARD.encode([1_u8; 256]))],
            "schedule": {"months": 3, "issued": 1},
        });
        Ok(json(200, &answer.to_string()))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Err(TokenError::Protocol(_))
    ));
    assert!(tokens.state().unwrap().tokens.is_empty());
    // No key for the tier: nothing is issued.
    let dir = TempDir::new("azcloud-period");
    let tokens = store_in(&dir).on_this_device("d_2").unwrap();
    tokens.keep_grant(VaultGrant::new("ck_1", "10TB", 3, ISSUE_KEY)).unwrap();
    let fake = issuing_server(keys(), N1, D1, "100GB/2026");
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Err(TokenError::Protocol(_))
    ));
    assert_eq!(fake.calls().len(), 1, "only the keys were asked for");
}

/// PT4: a payment taken back after the approval (a refund, a chargeback) ends the grant: said
/// once, never asked again.
#[test]
fn a_refusal_that_can_never_pass_ends_the_grant() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let keys = keys();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &keys));
        }
        Ok(json(409, r#"{"error": "not_paid", "message": "the checkout is not approved"}"#))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match issue_next(&server, &tokens, "ck_1", NOW).unwrap() {
        Issued::Ended(why) => assert!(why.contains("taken back"), "{why}"),
        other => panic!("not ended: {other:?}"),
    }
    assert!(tokens.state().unwrap().grant("ck_1").unwrap().ended.is_some());
    let calls = fake.calls().len();
    assert!(matches!(issue_next(&server, &tokens, "ck_1", NOW), Ok(Issued::Ended(_))));
    assert_eq!(fake.calls().len(), calls, "never asked again");
}

pub(crate) fn stored(nonce: &str) -> PeriodToken {
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

// ==== Redeeming: under the drive's lock, with its newest token ====

pub(crate) const NOW: u64 = 1_791_450_000;
pub(crate) const DAY: u64 = 86_400;

/// A keyring with drive `d_1`'s session (its drive token `token`), and its locks.
pub(crate) fn keyring_with_session(dir: &TempDir, token: &str) -> SharedKeyring {
    let keyring = Arc::new(MemoryKeyring::new());
    let session = AzlinSession {
        drive_id: String::from("d_1"),
        drive_token: crate::DriveToken::new(token),
        access_key_id: String::from("AKID1"),
        secret_access_key: String::from("secret"),
        session_token: None,
        expires_at: Some(NOW + 3600),
        grant: crate::Grant::ReadWrite,
    };
    keyring
        .set(&keyring_key("d_1"), &session.to_keyring_secret())
        .unwrap();
    SharedKeyring::with_lock_root(
        keyring,
        KeyringNamespace::azlin_shared(),
        &dir.path().join("locks"),
    )
}

/// A token server whose drive `d_1` (100 GB) is paid until `until` (seconds since 1970, moved
/// on by 30 days with every redemption answered by `redeem(nonce)`), that checks every call
/// carries `token` and comes while the drive's lock is held.
pub(crate) fn redeeming_server(
    locks: LockDir,
    token: &'static str,
    until: u64,
    redeem: impl Fn(&str) -> Option<HttpReply> + Send + Sync + 'static,
) -> Arc<Fake> {
    let until = Arc::new(AtomicU64::new(until));
    Fake::new(move |call, _| {
        assert_eq!(header(call, "authorization"), Some(format!("Bearer {token}").as_str()));
        assert!(
            locks.lock(&keyring_key("d_1"), Duration::ZERO).is_err(),
            "the drive's lock is held during {}",
            call.url
        );
        if call.method == Method::Get {
            assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1"));
            let answer = serde_json::json!({
                "id": "d_1", "tier": "100GB",
                "period_until": rfc3339(until.load(Ordering::SeqCst)),
            });
            return Ok(json(200, &answer.to_string()));
        }
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/redeem"));
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        if let Some(refusal) = redeem(body["nonce"].as_str().unwrap()) {
            return Ok(refusal);
        }
        let later = until.fetch_add(30 * DAY, Ordering::SeqCst) + 30 * DAY;
        let answer = serde_json::json!({"period_until": rfc3339(later)});
        Ok(json(200, &answer.to_string()))
    })
}

#[test]
fn a_period_token_is_redeemed_under_the_drives_lock_with_its_newest_token_when_the_period_nears_its_end(
) {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    store.add("d_1", &[stored("aa"), stored("bb")]).unwrap();
    let fake = redeeming_server(shared.locks().clone(), "dt_f.3.newest", NOW + 2 * DAY, |_| None);
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // Two days left: one token, a month more.
    match redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW) {
        Redeemed::Extended {
            count,
            period_until,
        } => assert_eq!((count, period_until), (1, Some(NOW + 32 * DAY))),
        other => panic!("not redeemed: {other:?}"),
    }
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("bb")], "the oldest went first");
    let calls = fake.calls();
    assert_eq!(calls.len(), 2, "the drive's period, then one redemption");
    let body: serde_json::Value = serde_json::from_slice(&calls[1].body).unwrap();
    assert_eq!(body["nonce"], "aa".repeat(32));
    // A month ahead: nothing to redeem yet; the token waits.
    match redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW) {
        Redeemed::NotDue { period_until } => assert_eq!(period_until, Some(NOW + 32 * DAY)),
        other => panic!("redeemed too early: {other:?}"),
    }
    assert_eq!(fake.calls().len(), 3);
    assert_eq!(store.tokens("d_1").unwrap().len(), 1);
}

#[test]
fn a_used_or_useless_period_token_is_dropped_and_the_next_one_redeemed() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.0.first");
    let store = store_in(&dir);
    store
        .add("d_1", &[stored("aa"), stored("bb"), stored("cc"), stored("dd")])
        .unwrap();
    let fake = redeeming_server(shared.locks().clone(), "dt_f.0.first", NOW + DAY, |nonce| {
        match &nonce[..2] {
            "aa" => Some(json(
                409,
                r#"{"error": "token_used", "message": "this token was already redeemed"}"#,
            )),
            "bb" => Some(json(
                400,
                r#"{"error": "bad_token", "message": "the token signature does not verify"}"#,
            )),
            "cc" => None,
            _ => Some(json(503, r#"{"error": "busy", "message": "try later"}"#)),
        }
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW) {
        Redeemed::Extended { count, .. } => assert_eq!(count, 1),
        other => panic!("not redeemed: {other:?}"),
    }
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("dd")], "used and useless ones gone");
    // A token server that cannot answer now: the token stays for the next try.
    let fake = redeeming_server(shared.locks().clone(), "dt_f.0.first", NOW + DAY, |_| {
        Some(json(503, r#"{"error": "busy", "message": "try later"}"#))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW),
        Redeemed::Kept(_)
    ));
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("dd")]);
}

/// ENT27 n51: a redemption refused in the contract's words `drive_banned` and `lockdown_pending`
/// (both 403) is read as that - a banned drive is closed, a pending lockdown makes the months
/// wait with its reason - and the token is kept in both. Before, the contract's catch-all took
/// them: the look kept the token as "waits for the next try" with the server's raw sentence.
#[test]
fn ent27_n51_a_redeem_refused_as_banned_or_as_locked_down_is_read_as_that() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.0.first");
    let store = store_in(&dir);
    store.add("d_1", &[stored("aa"), stored("bb")]).unwrap();
    let refused = |word: &'static str| {
        let fake = redeeming_server(shared.locks().clone(), "dt_f.0.first", NOW + DAY, move |_| {
            Some(json(
                403,
                &format!(r#"{{"error": "{word}", "message": "refused: {word}"}}"#),
            ))
        });
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW)
    };
    assert_eq!(refused("drive_banned"), Redeemed::Banned);
    match refused("lockdown_pending") {
        Redeemed::Kept(why) => assert!(why.contains("lockdown"), "{why}"),
        other => panic!("not a wait: {other:?}"),
    }
    assert_eq!(
        store.tokens("d_1").unwrap(),
        vec![stored("aa"), stored("bb")],
        "neither word spends a token"
    );
}

/// AZ-P16: a token server that lost an issuer key (or verifies with another) refuses EVERY token
/// as `unknown_issuer` / `bad_token`. One look gives up one token on that word and keeps the
/// rest for the next look, instead of draining the whole vault; a token that is really bad
/// (the refusals of the test above) costs nothing, the good ones behind it are redeemed.
#[test]
fn az_p16_a_server_that_refuses_every_token_as_unknown_issuer_loses_one_token_per_look() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.0.first");
    let store = store_in(&dir);
    store
        .add(
            "d_1",
            &[stored("aa"), stored("bb"), stored("cc"), stored("dd")],
        )
        .unwrap();
    let fake = redeeming_server(shared.locks().clone(), "dt_f.0.first", NOW + DAY, |_| {
        Some(json(
            400,
            r#"{"error": "unknown_issuer", "message": "no issuer key for the token's tier"}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW) {
        Redeemed::Kept(why) => assert!(why.contains("unknown_issuer"), "{why}"),
        other => panic!("a server that refuses every token redeemed nothing: {other:?}"),
    }
    assert_eq!(
        store.tokens("d_1").unwrap(),
        vec![stored("bb"), stored("cc"), stored("dd")],
        "the first was given up on its word, the second ended the look and stayed"
    );
    let asked = fake
        .calls()
        .iter()
        .filter(|c| c.url.ends_with("/redeem"))
        .count();
    assert_eq!(asked, 2, "no third token was asked about in this look");
    // The next look is the same: one token more, never the whole vault at once.
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let _ = redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW);
    assert_eq!(
        store.tokens("d_1").unwrap(),
        vec![stored("cc"), stored("dd")]
    );
}

#[test]
fn a_drive_without_kept_tokens_or_a_session_asks_the_token_server_nothing() {
    let dir = TempDir::new("azcloud-period");
    let store = store_in(&dir);
    let fake = Fake::new(|call, _| panic!("nothing is asked: {}", call.url));
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let shared = keyring_with_session(&dir, "dt_f.0.first");
    assert!(matches!(
        redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW),
        Redeemed::Nothing
    ));
    // Tokens, but this device has no session of the drive: kept, nothing sent.
    store.add("d_2", &[stored("aa")]).unwrap();
    assert!(matches!(
        redeem_due(&server, &shared, &store.on_this_device("d_2").unwrap(), NOW),
        Redeemed::Kept(_)
    ));
    assert_eq!(store.tokens("d_2").unwrap().len(), 1);
}

#[test]
fn a_drive_token_call_runs_under_the_drives_lock_with_the_newest_token() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.7.newest");
    let locks = shared.locks().clone();
    let seen = shared
        .with_drive_token("d_1", |token| {
            assert!(locks.lock(&keyring_key("d_1"), Duration::ZERO).is_err(), "held");
            token.to_string()
        })
        .unwrap();
    assert_eq!(seen, "dt_f.7.newest");
    assert!(locks.lock(&keyring_key("d_1"), Duration::ZERO).is_ok(), "released after");
    assert!(shared.with_drive_token("d_2", |_| ()).is_err(), "no session of d_2");
}

#[test]
fn a_look_names_the_members_this_device_had_not_seen_before() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let fake = Fake::new(|_, n| {
        let members: &[&str] = match n {
            0 => &["owner"],
            1 | 2 => &["owner", "m_laptop", "m_phone"],
            _ => &["owner", "m_laptop", "m_phone", "recovery-pending"],
        };
        let answer = serde_json::json!({
            "id": "d_1", "tier": "100GB", "you": "m_phone",
            "period_until": rfc3339(NOW + 20 * DAY),
            "members": members.iter().map(|m| serde_json::json!({
                "member": m, "role": "member", "added_at": rfc3339(NOW),
            })).collect::<Vec<_>>(),
        });
        Ok(json(200, &answer.to_string()))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_1").unwrap(), NOW);
    assert!(look.new_members.is_empty(), "the first look learns them");
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_1").unwrap(), NOW);
    assert_eq!(
        look.new_members,
        vec![String::from("m_laptop")],
        "this device itself is no news"
    );
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_1").unwrap(), NOW);
    assert!(look.new_members.is_empty(), "seen");
    // Kept on disk: another store on the same folder (AzDrive started again) knows them.
    let look = look_at_drive(&server, &shared, &store_in(&dir), &store.on_this_device("d_1").unwrap(), NOW);
    assert_eq!(look.new_members, vec![String::from(RECOVERY_MEMBER)]);
}

#[test]
fn a_look_at_a_drive_reads_its_status_even_without_kept_tokens_and_redeems_when_due() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    // No token kept: the status still comes (a pending lockdown is news to the owner).
    let fake = Fake::new(|call, _| {
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1"));
        assert_eq!(header(call, "authorization"), Some("Bearer dt_f.3.newest"));
        let answer = serde_json::json!({
            "id": "d_1", "tier": "100GB",
            "period_until": rfc3339(NOW + 20 * DAY),
            "lockdown_pending_until": rfc3339(NOW + 2 * DAY),
        });
        Ok(json(200, &answer.to_string()))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_1").unwrap(), NOW);
    assert_eq!(look.redeemed, Redeemed::Nothing);
    let status = look.status.expect("the drive's status");
    assert_eq!(status.lockdown_pending_until, Some(NOW + 2 * DAY));
    assert_eq!(status.period_until, Some(NOW + 20 * DAY));
    // A kept token and a period due: redeemed in the same look; the status has the new end.
    store.add("d_1", &[stored("aa")]).unwrap();
    let fake = redeeming_server(shared.locks().clone(), "dt_f.3.newest", NOW + 2 * DAY, |_| None);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_1").unwrap(), NOW);
    assert!(
        matches!(look.redeemed, Redeemed::Extended { count: 1, .. }),
        "{:?}",
        look.redeemed
    );
    assert_eq!(look.status.unwrap().period_until, Some(NOW + 32 * DAY));
    // No session of the drive: no status, kept for the next look.
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_2").unwrap(), NOW);
    assert!(look.status.is_none() && matches!(look.redeemed, Redeemed::Kept(_)));
}

/// D3: a banned drive cannot use a month - its look buys none, and its tokens stay.
#[test]
fn a_banned_drive_buys_no_month() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    store.add("d_1", &[stored("aa")]).unwrap();
    let fake = Fake::new(|call, _| {
        assert_eq!(call.method, Method::Get, "nothing is redeemed: {}", call.url);
        let answer = serde_json::json!({
            "id": "d_1", "tier": "100GB", "period_until": rfc3339(NOW + DAY),
            "status": "banned", "ban_reason": "spam distribution", "ban_until": rfc3339(NOW + 2 * DAY),
        });
        Ok(json(200, &answer.to_string()))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_1").unwrap(), NOW);
    assert_eq!(look.redeemed, Redeemed::Banned);
    assert!(look.grant_changed.is_none());
    assert_eq!(store.tokens("d_1").unwrap(), vec![stored("aa")], "the token stays");
}

/// PT3: a month bought says the drive's grant changed - what makes its read-only credentials
/// refresh before the next write.
#[test]
fn a_month_bought_says_the_grant_changed() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    store.add("d_1", &[stored("aa")]).unwrap();
    let fake = redeeming_server(shared.locks().clone(), "dt_f.3.newest", NOW + 2 * DAY, |_| None);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = look_at_drive(&server, &shared, &store, &store.on_this_device("d_1").unwrap(), NOW);
    assert!(matches!(look.redeemed, Redeemed::Extended { count: 1, .. }), "{:?}", look.redeemed);
    assert_eq!(look.grant_changed.as_ref().map(|g| g.drive_id()), Some("d_1"));
}

/// Decision 6: a drive that needs a month and holds no token gets its grant's next month issued
/// in the same look, and redeemed.
#[test]
fn a_drive_that_needs_a_month_and_holds_none_gets_its_grants_next_month() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    let mut grant = VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY);
    grant.issued = 1;
    grant.next_issue_at = Some(NOW - 60);
    tokens.keep_grant(grant).unwrap();
    let keys = keys();
    let issued = AtomicU64::new(1);
    let until = AtomicU64::new(NOW + 2 * DAY);
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &keys));
        }
        if call.url.ends_with("/v1/tokens/issue") {
            return Ok(signed(call, N1, D1, "100GB/2026", &issued));
        }
        if call.method == Method::Get {
            let answer = serde_json::json!({
                "id": "d_1", "tier": "100GB", "period_until": rfc3339(until.load(Ordering::SeqCst)),
            });
            return Ok(json(200, &answer.to_string()));
        }
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/redeem"));
        let later = until.fetch_add(30 * DAY, Ordering::SeqCst) + 30 * DAY;
        Ok(json(200, &serde_json::json!({"period_until": rfc3339(later)}).to_string()))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = look_at_drive(&server, &shared, &store, &tokens, NOW);
    assert!(matches!(look.redeemed, Redeemed::Extended { count: 1, .. }), "{:?}", look.redeemed);
    let state = tokens.state().unwrap();
    assert!(state.tokens.is_empty(), "issued and redeemed");
    assert_eq!(state.grant("ck_1").unwrap().issued, 2);
}

// ==== AZ-P17: an end another device wrote does not lose this device's counted month ====

/// Drive `d_1`'s grant `ck_1` (3 months, 2 issued, the last one due): THIS device asked for the
/// last month and the token server counted it, but the answer was lost - the request stays kept.
/// Then ANOTHER device, whose own request for that month was refused as "already issued", ended
/// the grant: the end is in every vault. The grant's end text and the first (lost) request's
/// body.
fn last_month_lost_then_ended(dir: &TempDir) -> (DriveTokens, String, Vec<u8>) {
    let tokens = store_in(dir).on_this_device("d_1").unwrap();
    let mut grant = VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY);
    grant.issued = 2;
    grant.next_issue_at = Some(NOW - 60);
    tokens.keep_grant(grant.clone()).unwrap();
    let listed = keys();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        Err(String::from("connection reset"))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Err(TokenError::Connect(_))
    ));
    assert_eq!(
        tokens.state().unwrap().open_issues("ck_1").len(),
        1,
        "the request, kept"
    );
    let first = fake.calls()[1].body.clone();
    let why = ended_words(&IssueRefusal::AlreadyIssued);
    let mut ended = grant;
    ended.ended = Some(why.clone());
    tokens.join(&VaultState::with_grant(ended)).unwrap();
    let state = tokens.state().unwrap();
    assert!(
        !state.grant("ck_1").unwrap().is_open(),
        "ended in the vault"
    );
    assert_eq!(
        state.open_issues("ck_1").len(),
        1,
        "the request is still kept"
    );
    (tokens, why, first)
}

/// AZ-P17: a grant ended by another device is not the end of THIS device's counted request:
/// the request is sent again as it is (the token server answers it with the same signatures,
/// counted once), the month is kept, and the first end stands. Before, the end was checked
/// first, the request never replayed, and that month was lost.
#[test]
fn az_p17_a_grant_ended_by_another_device_still_replays_this_devices_counted_request() {
    let dir = TempDir::new("azcloud-period");
    let (tokens, why, first) = last_month_lost_then_ended(&dir);
    // The token server counted the request: the replay is answered with the month's signatures.
    let issued = AtomicU64::new(2);
    let listed = keys();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        Ok(signed(call, N1, D1, "100GB/2026", &issued))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match issue_next(&server, &tokens, "ck_1", NOW + 31 * DAY).unwrap() {
        Issued::Tokens(issued) => assert_eq!(issued.len(), 1),
        other => panic!("the counted month was lost to the other device's end: {other:?}"),
    }
    let calls = fake.calls();
    assert_eq!(calls.len(), 1, "the replay only: no keys, no new blinding");
    assert_eq!(calls[0].body, first, "the identical request");
    let state = tokens.state().unwrap();
    assert_eq!(state.tokens.len(), 1, "the month is kept");
    assert!(state.issues.is_empty(), "the request is answered");
    let grant = state.grant("ck_1").unwrap();
    assert_eq!(grant.issued, 3);
    assert_eq!(grant.ended.as_deref(), Some(why.as_str()), "the end stands");
    // Nothing waits any more: the ended grant is not asked again.
    match issue_next(&server, &tokens, "ck_1", NOW + 62 * DAY).unwrap() {
        Issued::Ended(said) => assert_eq!(said, why),
        other => panic!("not ended: {other:?}"),
    }
    assert_eq!(fake.calls().len(), 1, "never asked again");
}

/// AZ-P17: a replay the token server refuses for good closes the request - one replay, never a
/// loop - and the end that was there first is the one said and kept.
#[test]
fn az_p17_a_replay_refused_for_good_closes_the_request_and_the_first_end_stands() {
    let dir = TempDir::new("azcloud-period");
    let (tokens, why, _) = last_month_lost_then_ended(&dir);
    let fake = Fake::new(|_, _| {
        Ok(json(
            409,
            r#"{"error": "not_paid", "message": "the checkout is not approved"}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    match issue_next(&server, &tokens, "ck_1", NOW + 31 * DAY).unwrap() {
        Issued::Ended(said) => assert_eq!(said, why, "the first end is the one said"),
        other => panic!("not ended: {other:?}"),
    }
    let state = tokens.state().unwrap();
    assert!(state.issues.is_empty(), "the request is closed");
    assert!(state.tokens.is_empty());
    assert_eq!(
        state.grant("ck_1").unwrap().ended.as_deref(),
        Some(why.as_str())
    );
    assert_eq!(fake.calls().len(), 1);
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 62 * DAY),
        Ok(Issued::Ended(_))
    ));
    assert_eq!(fake.calls().len(), 1, "never asked again");
}

/// AZ-P17: the look at the drive reaches the replay too - a drive whose only grant is ended but
/// holds a request waiting for its answer still "holds a month" (before, `holds_months` read the
/// ended grant as nothing and the look asked nothing) - and redeems the month it brings.
#[test]
fn az_p17_a_look_replays_the_request_of_an_ended_grant_and_redeems_its_month() {
    let dir = TempDir::new("azcloud-period");
    let (tokens, _why, first) = last_month_lost_then_ended(&dir);
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let listed = keys();
    let issued = AtomicU64::new(2);
    let until = AtomicU64::new(NOW + 2 * DAY);
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        if call.url.ends_with("/v1/tokens/issue") {
            return Ok(signed(call, N1, D1, "100GB/2026", &issued));
        }
        if call.method == Method::Get {
            let answer = serde_json::json!({
                "id": "d_1", "tier": "100GB", "period_until": rfc3339(until.load(Ordering::SeqCst)),
            });
            return Ok(json(200, &answer.to_string()));
        }
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/redeem"));
        let later = until.fetch_add(30 * DAY, Ordering::SeqCst) + 30 * DAY;
        Ok(json(200, &serde_json::json!({"period_until": rfc3339(later)}).to_string()))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let look = look_at_drive(&server, &shared, &store, &tokens, NOW);
    assert!(
        matches!(look.redeemed, Redeemed::Extended { count: 1, .. }),
        "{:?}",
        look.redeemed
    );
    let replays: Vec<_> = fake
        .calls()
        .into_iter()
        .filter(|c| c.url.ends_with("/v1/tokens/issue"))
        .collect();
    assert_eq!(replays.len(), 1);
    assert_eq!(replays[0].body, first, "the identical request");
    let state = tokens.state().unwrap();
    assert!(state.tokens.is_empty(), "issued and redeemed");
    assert!(state.issues.is_empty());
    assert_eq!(state.grant("ck_1").unwrap().issued, 3);
    // The next look: the ended grant holds nothing; the issue endpoint is not asked again.
    let look = look_at_drive(&server, &shared, &store, &tokens, NOW);
    assert_eq!(look.redeemed, Redeemed::Nothing);
    let asked = fake
        .calls()
        .iter()
        .filter(|c| c.url.ends_with("/v1/tokens/issue"))
        .count();
    assert_eq!(asked, 1, "never asked again");
}
