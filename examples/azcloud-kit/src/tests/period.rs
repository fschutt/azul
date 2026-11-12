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
pub(crate) fn signed(
    call: &azul_storage::HttpCall,
    n: &str,
    d: &str,
    key_id: &str,
    issued: &AtomicU64,
) -> HttpReply {
    signed_for(call, "ck_1", n, d, key_id, issued)
}

/// [`signed`] for the checkout `checkout_id` (its issue key is `ISSUE_KEY` too).
fn signed_for(
    call: &azul_storage::HttpCall,
    checkout_id: &str,
    n: &str,
    d: &str,
    key_id: &str,
    issued: &AtomicU64,
) -> HttpReply {
    assert_eq!(call.method, Method::Post);
    assert_eq!(call.url, format!("{TOKEN}/v1/tokens/issue"));
    assert_eq!(header(call, "authorization"), None, "no drive token");
    let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    assert_eq!(body["checkout_id"], checkout_id);
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

/// X-DELTA-AZDRIVE-MONEY-4: a refusal about the REQUEST - `bad_request` (a body the token server
/// could not read: a proxy that empties a POST body; an error inside its signing), no issue key,
/// no issuer key - says nothing of the checkout, so it never ends the grant: the months not
/// issued yet stay, the issue key with them, the request is kept and goes again at the next look,
/// the error says to update the app. Before, the three words were "final": the grant was marked
/// ended in every vault and the keyring's copy of the issue key was erased.
#[test]
fn x_delta_azdrive_money_4_a_refusal_about_the_request_keeps_the_grant_and_its_request() {
    for word in ["bad_request", "issue_key_required", "key_id_required"] {
        let dir = TempDir::new("azcloud-period");
        let tokens = granted(&dir);
        let listed = keys();
        let issued = AtomicU64::new(0);
        let fake = Fake::new(move |call, n| {
            if call.url.ends_with("/v1/tokens/keys") {
                return Ok(json(200, &listed));
            }
            if n == 1 {
                let body = format!(r#"{{"error": "{word}", "message": "the body did not read"}}"#);
                return Ok(json(400, &body));
            }
            Ok(signed(call, N1, D1, "100GB/2026", &issued))
        });
        let transport = Shared(fake.clone());
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        match issue_next(&server, &tokens, "ck_1", NOW) {
            Err(TokenError::Refused {
                status,
                code,
                message,
            }) => {
                assert_eq!((status, code.as_str()), (400, word));
                assert!(message.contains("update the app"), "{word}: {message}");
            }
            other => panic!("{word}: not an error about the request: {other:?}"),
        }
        let state = tokens.state().unwrap();
        assert!(
            state
                .grant("ck_1")
                .is_some_and(|g| g.ended.is_none() && g.is_open()),
            "{word}: the grant is not ended"
        );
        assert_eq!(
            state.open_issues("ck_1").len(),
            1,
            "{word}: the request is kept"
        );
        // The next look sends the very same request and gets the month.
        match issue_next(&server, &tokens, "ck_1", NOW + DAY).unwrap() {
            Issued::Tokens(issued) => assert_eq!(issued.len(), 1, "{word}"),
            other => panic!("{word}: the month was lost: {other:?}"),
        }
        let calls = fake.calls();
        let issues: Vec<_> = calls
            .iter()
            .filter(|c| c.url.ends_with("/v1/tokens/issue"))
            .collect();
        assert_eq!(issues.len(), 2, "{word}");
        assert_eq!(
            issues[0].body, issues[1].body,
            "{word}: the identical request"
        );
    }
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

/// AZ-P16: a token server that lost an issuer key (or verifies with another, or has its issuer
/// year wrong) refuses EVERY token as `unknown_issuer` / `bad_token` / `token_expired`. Nothing
/// is given up on that word: the whole vault stays, the look stops after three questions, and
/// the next look is the same. (The tokens and the questions are asserted first, the server's own
/// sentence after - X-DELTA-AZDRIVE-MONEY-14: the text is the server's `message`, not its word.)
/// Before, the first refusal of every look spent a paid token: a month short per day the fault
/// lasted (X-DELTA-AZDRIVE-MONEY-3).
#[test]
fn az_p16_a_server_that_refuses_every_token_on_a_doubtful_word_loses_no_token() {
    for (word, message) in [
        ("unknown_issuer", "no issuer key for the token's tier"),
        ("bad_token", "the token signature does not verify"),
        ("token_expired", "the token's issuer year is not valid"),
    ] {
        let dir = TempDir::new("azcloud-period");
        let shared = keyring_with_session(&dir, "dt_f.0.first");
        let store = store_in(&dir);
        let all = [stored("aa"), stored("bb"), stored("cc"), stored("dd")];
        store.add("d_1", &all).unwrap();
        let fake = redeeming_server(
            shared.locks().clone(),
            "dt_f.0.first",
            NOW + DAY,
            move |_| {
                Some(json(
                    400,
                    &format!(r#"{{"error": "{word}", "message": "{message}"}}"#),
                ))
            },
        );
        let transport = Shared(fake.clone());
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        let look = redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW);
        assert_eq!(
            store.tokens("d_1").unwrap(),
            all.to_vec(),
            "{word}: no token is given up on a word the server may have said of every token"
        );
        let asked = fake
            .calls()
            .iter()
            .filter(|c| c.url.ends_with("/redeem"))
            .count();
        assert_eq!(asked, 3, "{word}: three questions, then the look ends");
        match look {
            Redeemed::Kept(why) => assert!(why.contains(message), "{word}: {why}"),
            other => {
                panic!("{word}: a server that refuses every token redeemed nothing: {other:?}")
            }
        }
        // The next look is the same: still the whole vault.
        let _ = redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW);
        assert_eq!(
            store.tokens("d_1").unwrap(),
            all.to_vec(),
            "{word}: the next look"
        );
    }
}

/// The token server's 400 `word` to a redemption.
fn refused_as(word: &str) -> HttpReply {
    json(
        400,
        &format!(r#"{{"error": "{word}", "message": "refused"}}"#),
    )
}

/// X-DELTA-AZDRIVE-MONEY-3 (AZ-P16): a token refused on a doubtful word is given up on when - and
/// only when - the token server takes another token that shows the fault is not its own: one of
/// the same tier and issuer year for `unknown_issuer` / `bad_token`; for `token_expired` one of
/// the same tier whose year is the same or LATER (an older year taken says nothing of a newer
/// year's refusal). Without such a token the refused one stays. (The drive is 40 days behind, so
/// the look goes on after a redemption.)
#[test]
fn x_delta_azdrive_money_3_a_refused_token_is_given_up_on_only_when_the_server_takes_another_of_its_issuer(
) {
    let in_year = |nonce: &str, year: u32| PeriodToken {
        year,
        ..stored(nonce)
    };
    let look = |tokens: &[PeriodToken], answer: fn(&str) -> Option<HttpReply>| {
        let dir = TempDir::new("azcloud-period");
        let shared = keyring_with_session(&dir, "dt_f.0.first");
        let store = store_in(&dir);
        store.add("d_1", tokens).unwrap();
        let fake = redeeming_server(
            shared.locks().clone(),
            "dt_f.0.first",
            NOW - 40 * DAY,
            answer,
        );
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        let redeemed = redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW);
        (redeemed, store.tokens("d_1").unwrap())
    };
    // A bad token ahead of a good one of its year: refused and kept, spent once the good one is taken.
    let (redeemed, left) = look(&[stored("aa"), stored("bb")], |nonce| {
        (&nonce[..2] == "aa").then(|| refused_as("bad_token"))
    });
    assert!(
        matches!(redeemed, Redeemed::Extended { count: 1, .. }),
        "{redeemed:?}"
    );
    assert!(
        left.is_empty(),
        "the bad one was settled by the good one: {left:?}"
    );
    // The other way round: the server took one of the year first, so the refusal is the token's.
    let (_, left) = look(&[stored("bb"), stored("aa")], |nonce| {
        (&nonce[..2] == "aa").then(|| refused_as("unknown_issuer"))
    });
    assert!(left.is_empty(), "{left:?}");
    // Nothing taken afterwards: the refused one stays, and so does the one that met a busy server.
    let (redeemed, left) = look(&[stored("aa"), stored("bb")], |nonce| {
        if &nonce[..2] == "aa" {
            Some(refused_as("bad_token"))
        } else {
            Some(json(503, r#"{"error": "busy", "message": "try later"}"#))
        }
    });
    assert!(matches!(redeemed, Redeemed::Kept(_)), "{redeemed:?}");
    assert_eq!(
        left,
        vec![stored("aa"), stored("bb")],
        "none was given up on"
    );
    // The year: a token of 2026 refused as expired is bad once a token of 2027 is taken ...
    let (redeemed, left) = look(&[in_year("aa", 2026), in_year("bb", 2027)], |nonce| {
        (&nonce[..2] == "aa").then(|| refused_as("token_expired"))
    });
    assert!(
        matches!(redeemed, Redeemed::Extended { .. }),
        "{redeemed:?}"
    );
    assert!(
        left.is_empty(),
        "an older year is expired once a newer one is taken: {left:?}"
    );
    // ... but a token of 2027 refused as expired is not bad because one of 2026 was taken: the
    // server's own year may be behind.
    let (redeemed, left) = look(&[in_year("aa", 2027), in_year("bb", 2026)], |nonce| {
        (&nonce[..2] == "aa").then(|| refused_as("token_expired"))
    });
    assert!(
        matches!(redeemed, Redeemed::Extended { .. }),
        "{redeemed:?}"
    );
    assert_eq!(
        left,
        vec![in_year("aa", 2027)],
        "a newer year is not settled by an older one"
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

// ==== X-DELTA-MONEY2-10: a request the token server keeps refusing as one it cannot take ====

/// The messages of an issue request's body.
fn blinded_of(call: &azul_storage::HttpCall) -> Vec<String> {
    let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
    body["blinded"]
        .as_array()
        .unwrap()
        .iter()
        .map(|b| b.as_str().unwrap().to_string())
        .collect()
}

/// X-DELTA-MONEY2-10: a kept issue request that the token server refuses as `bad_request` (a
/// message it cannot sign, a proxy that keeps emptying the POST body) was sent again at every look
/// and no fresh request was ever built - the grant never ended and never recovered (the old code,
/// before X-DELTA-AZDRIVE-MONEY-4, ended the grant: no worse, but no better). Now the third
/// identical refusal ends THAT request - the token server signed and counted nothing - and the same
/// look blinds a new one, which the next look sends and the server signs when it can. The count is
/// the request's, kept in every vault of the drive.
#[test]
fn x_delta_money2_10_a_request_refused_three_times_as_bad_request_is_given_up_and_a_new_one_is_blinded(
) {
    use crate::vault::TokenVault;
    let dir = TempDir::new("azcloud-period");
    let store = store_in(&dir);
    let (tokens, in_drive) = super::tokens_in_drive(&store);
    tokens
        .keep_grant(VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY))
        .unwrap();
    let listed = keys();
    let issued = AtomicU64::new(0);
    let healthy = Arc::new(AtomicUsize::new(0));
    let well = healthy.clone();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        if well.load(Ordering::SeqCst) == 1 {
            return Ok(signed(call, N1, D1, "100GB/2026", &issued));
        }
        Ok(json(
            400,
            r#"{"error": "bad_request", "message": "blind sign: no"}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let posts = || {
        fake.calls()
            .into_iter()
            .filter(|c| c.url.ends_with("/v1/tokens/issue"))
            .collect::<Vec<_>>()
    };

    // Looks one and two: the request is made, kept and refused; the same one goes again.
    for look in 1..=2 {
        assert!(
            issue_next(&server, &tokens, "ck_1", NOW + look * DAY).is_err(),
            "look {look}"
        );
        let state = in_drive.read().unwrap();
        let open = state.open_issues("ck_1");
        assert_eq!(open.len(), 1, "look {look}: the request is kept");
        assert_eq!(
            open[0].refusals(),
            u32::try_from(look).unwrap(),
            "in the drive's vault too"
        );
    }
    assert_eq!(
        blinded_of(&posts()[0]),
        blinded_of(&posts()[1]),
        "the same request again"
    );

    // Look three: the third identical refusal ends the request and the look blinds another.
    assert!(issue_next(&server, &tokens, "ck_1", NOW + 3 * DAY).is_err());
    let sent = posts();
    assert_eq!(
        sent.len(),
        4,
        "the kept one a third time, then a new one in the same look"
    );
    assert_eq!(
        blinded_of(&sent[1]),
        blinded_of(&sent[2]),
        "the third try is the same request"
    );
    assert_ne!(
        blinded_of(&sent[2]),
        blinded_of(&sent[3]),
        "its third refusal ended it: the next message is a new one"
    );
    let state = tokens.state().unwrap();
    assert_eq!(state.done.len(), 1, "the refused request is marked done");
    let open = state.open_issues("ck_1");
    assert_eq!(open.len(), 1, "and the new one is kept");
    assert_eq!(open[0].refusals(), 1, "with a count of its own");
    assert!(
        state
            .grant("ck_1")
            .is_some_and(|g| g.ended.is_none() && g.is_open()),
        "the grant stays as it was"
    );

    // The token server can sign again: the next look sends the new request, and the month comes.
    healthy.store(1, Ordering::SeqCst);
    match issue_next(&server, &tokens, "ck_1", NOW + 4 * DAY).unwrap() {
        Issued::Tokens(tokens) => assert_eq!(tokens.len(), 1),
        other => panic!("the month did not come: {other:?}"),
    }
    let sent = posts();
    assert_eq!(sent.len(), 5);
    assert_eq!(
        blinded_of(&sent[3]),
        blinded_of(&sent[4]),
        "the new request, as it was kept"
    );
}

/// "Identical" refusals: the count is by the word of the refusal - a request refused as
/// `bad_request` twice and as `key_id_required` twice is not given up (three different complaints
/// are not three of one), and the third `bad_request` ends it.
#[test]
fn x_delta_money2_10_the_refusals_that_end_a_request_are_identical_ones() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let listed = keys();
    let posted = AtomicUsize::new(0);
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        // the words, one per issue call
        let word = [
            "bad_request",
            "bad_request",
            "key_id_required",
            "key_id_required",
            "bad_request",
        ][posted.fetch_add(1, Ordering::SeqCst).min(4)];
        Ok(json(
            400,
            &format!(r#"{{"error": "{word}", "message": "no"}}"#),
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    for look in 1..=4 {
        let _ = issue_next(&server, &tokens, "ck_1", NOW);
        let state = tokens.state().unwrap();
        assert_eq!(
            state.done.len(),
            0,
            "look {look}: no word was said three times"
        );
        assert_eq!(
            state.open_issues("ck_1")[0].refusals(),
            if look == 1 { 1 } else { 2 }
        );
    }
    // the third bad_request, whatever came between: that request is given up (and a new one kept)
    let _ = issue_next(&server, &tokens, "ck_1", NOW);
    let state = tokens.state().unwrap();
    assert_eq!(state.done.len(), 1, "given up");
    assert_eq!(
        state.open_issues("ck_1")[0].refusals(),
        1,
        "a new request, a new count"
    );
}

// ==== X-DELTA-MONEY2-11: tokens a healthy server calls expired ====

/// A token of the issuer year `year`.
fn of_year(nonce: &str, year: u32) -> PeriodToken {
    PeriodToken {
        year,
        ..stored(nonce)
    }
}

/// X-DELTA-MONEY2-11: three tokens older than the token server redeems (issued in 2021, it is
/// 2026) sitting ahead of a good one were asked, kept as doubts and asked again at every look:
/// three doubts end a look, so the good one was never reached - the old code spent them in turn.
/// A token the server calls expired that this computer's own clock calls expired too (its issuer
/// year is more than three years behind) is spent at once and does not count as a doubt.
#[test]
fn x_delta_money2_11_three_expired_tokens_ahead_of_a_good_one_do_not_end_the_look_before_it() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.0.first");
    let store = store_in(&dir);
    store
        .add(
            "d_1",
            &[
                of_year("aa", 2021),
                of_year("bb", 2021),
                of_year("cc", 2021),
                stored("dd"),
            ],
        )
        .unwrap();
    let fake = redeeming_server(
        shared.locks().clone(),
        "dt_f.0.first",
        NOW - 40 * DAY,
        |nonce| (&nonce[..2] != "dd").then(|| refused_as("token_expired")),
    );
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let redeemed = redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW);
    assert!(
        matches!(redeemed, Redeemed::Extended { count: 1, .. }),
        "{redeemed:?}"
    );
    assert!(
        store.tokens("d_1").unwrap().is_empty(),
        "the three expired ones are gone and the good one bought its month"
    );
    let asked = fake
        .calls()
        .iter()
        .filter(|c| c.url.ends_with("/redeem"))
        .count();
    assert_eq!(asked, 4, "each token was asked about once");
}

/// The negative controls of the same rule: a token that is NOT behind by more than three years
/// and is refused as expired is still the server's doubt (its issuer year may be set wrong); an old
/// token refused on a word that is not the year's (`unknown_issuer`) is a doubt too - only the
/// year's refusal is judged by the clock - and one three years behind exactly is not yet expired.
#[test]
fn x_delta_money2_11_only_a_year_refusal_of_a_token_the_clock_calls_expired_is_given_up() {
    let look = |tokens: &[PeriodToken], word: &'static str| {
        let dir = TempDir::new("azcloud-period");
        let shared = keyring_with_session(&dir, "dt_f.0.first");
        let store = store_in(&dir);
        store.add("d_1", tokens).unwrap();
        let fake = redeeming_server(
            shared.locks().clone(),
            "dt_f.0.first",
            NOW - 40 * DAY,
            move |_| Some(refused_as(word)),
        );
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        let _ = redeem_due(&server, &shared, &store.on_this_device("d_1").unwrap(), NOW);
        store.tokens("d_1").unwrap()
    };
    // 2023 + 3 = 2026: not behind by more than three years - the server may be wrong
    assert_eq!(
        look(&[of_year("aa", 2023)], "token_expired").len(),
        1,
        "three years: kept"
    );
    assert_eq!(
        look(&[of_year("aa", 2026)], "token_expired").len(),
        1,
        "this year: kept"
    );
    // the issuer's key, not its year: only a token that is surely bad is spent
    assert_eq!(
        look(&[of_year("aa", 2021)], "unknown_issuer").len(),
        1,
        "another word: kept"
    );
    assert_eq!(
        look(&[of_year("aa", 2021)], "bad_token").len(),
        1,
        "another word: kept"
    );
    // four years behind, and the server says so too: gone
    assert!(
        look(&[of_year("aa", 2022)], "token_expired").is_empty(),
        "four years: spent"
    );
}

// ==== X-DELTA-MONEY3-1: a payment taken back that the token server put back ====

/// The token server of `ck_1` answering every issue request as `mode` says - 0: `not_paid` (the
/// checkout was reversed), 1: it signs (the restore approved the checkout again), 2: `not_due`
/// (approved again, the next month later), 3: `mandate_stopped`, 4: `already_issued`, 5:
/// `issue_key_wrong`, 6: `no_such_checkout` - and counting the issue requests it was asked.
fn restoring_server(mode: &Arc<AtomicUsize>) -> (Arc<Fake>, Arc<AtomicUsize>) {
    let listed = keys();
    let issued = AtomicU64::new(0);
    let asked = Arc::new(AtomicUsize::new(0));
    let (counted, mode) = (asked.clone(), mode.clone());
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        counted.fetch_add(1, Ordering::SeqCst);
        Ok(match mode.load(Ordering::SeqCst) {
            0 => json(409, r#"{"error": "not_paid", "message": "the checkout is not approved"}"#),
            1 => signed(call, N1, D1, "100GB/2026", &issued),
            2 => json(
                409,
                &serde_json::json!({"error": "not_due", "message": "later",
                    "schedule": {"months": 3, "issued": 1, "next_issue_at": rfc3339(NOW + 40 * DAY)}})
                .to_string(),
            ),
            3 => json(409, r#"{"error": "mandate_stopped", "message": "stopped"}"#),
            4 => json(409, r#"{"error": "already_issued", "message": "all out"}"#),
            5 => json(403, r#"{"error": "issue_key_wrong", "message": "not this key"}"#),
            _ => json(404, r#"{"error": "no_such_checkout", "message": "gone"}"#),
        })
    });
    (fake, asked)
}

/// X-DELTA-MONEY3-1: the token server's restore approves a wrongly reversed checkout again, but the
/// client ended the grant for good on the first `NotPaid` - the contract calls it final, and nothing
/// in the vaults ever cleared an end - so an app that looked between the reversal and the restore
/// never got months 2 to 12. Now a grant ended because the payment was taken back is asked again,
/// at most once a week: while the checkout is reversed nothing is asked within the week; after the
/// restore the next ask is signed, the end - and the sentence that said the payment was taken back -
/// is cleared in the vault, and the month is issued. On the old code the grant stays ended and the
/// look a week later asks nothing.
#[test]
fn x_delta_money3_1_a_grant_ended_by_a_payment_taken_back_issues_its_month_after_the_restore() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let mode = Arc::new(AtomicUsize::new(0));
    let (fake, asked) = restoring_server(&mode);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // the payment is taken back (wrongly); the look that learns it ends the grant and says why
    match issue_next(&server, &tokens, "ck_1", NOW) {
        Ok(Issued::Ended(why)) => assert!(why.contains("taken back"), "{why}"),
        other => panic!("not ended: {other:?}"),
    }
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    let state = tokens.state().unwrap();
    let grant = state.grant("ck_1").unwrap();
    assert!(grant.is_ended() && grant.revivable, "{grant:?}");
    assert!(!grant.is_open());
    // the operator restores the checkout; the looks of the week ask nothing
    mode.store(1, Ordering::SeqCst);
    for day in [1, 3, 6] {
        assert!(
            matches!(
                issue_next(&server, &tokens, "ck_1", NOW + day * DAY),
                Ok(Issued::Ended(_))
            ),
            "day {day}"
        );
    }
    assert_eq!(asked.load(Ordering::SeqCst), 1, "not asked within the week");
    // a week on: asked, signed - the month comes and the grant lives again
    match issue_next(&server, &tokens, "ck_1", NOW + 8 * DAY) {
        Ok(Issued::Tokens(issued)) => assert_eq!(issued.len(), 1),
        other => panic!("the month did not come: {other:?}"),
    }
    let state = tokens.state().unwrap();
    let grant = state.grant("ck_1").unwrap();
    assert!(
        grant.ended.is_none() && !grant.is_ended(),
        "the end is cleared: {grant:?}"
    );
    assert!(grant.is_open() && !grant.revivable);
    assert_eq!(grant.issued, 1);
    assert_eq!(state.tokens.len(), 1, "the month's token is kept");
    assert_eq!(asked.load(Ordering::SeqCst), 2);
}

/// While the checkout stays reversed the ask is repeated a week apart - never within it.
#[test]
fn x_delta_money3_1_a_revivable_end_that_is_refused_again_is_asked_again_a_week_later() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let mode = Arc::new(AtomicUsize::new(0));
    let (fake, asked) = restoring_server(&mode);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Ok(Issued::Ended(_))
    ));
    // a week and a day: asked again, refused again
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 8 * DAY),
        Ok(Issued::Ended(_))
    ));
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    // the week after THAT ask is counted from it, not from the end
    for day in [9, 12, 14] {
        assert!(matches!(
            issue_next(&server, &tokens, "ck_1", NOW + day * DAY),
            Ok(Issued::Ended(_))
        ));
    }
    assert_eq!(asked.load(Ordering::SeqCst), 2, "a week apart");
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 16 * DAY),
        Ok(Issued::Ended(_))
    ));
    assert_eq!(asked.load(Ordering::SeqCst), 3);
    // the restore comes, and the next ask is signed
    mode.store(1, Ordering::SeqCst);
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 24 * DAY),
        Ok(Issued::Tokens(_))
    ));
}

/// Only a payment taken back and a mandate stopped for it (X-DELTA-MONEY6-1: the restore lifts
/// both) are asked again: every month issued, a wrong key and a checkout the token server does not
/// have end the grant for good - never asked, a week or a year on.
#[test]
fn x_delta_money3_1_any_other_final_refusal_is_never_asked_again() {
    for mode_word in [4, 5, 6] {
        let dir = TempDir::new("azcloud-period");
        let tokens = granted(&dir);
        let mode = Arc::new(AtomicUsize::new(mode_word));
        let (fake, asked) = restoring_server(&mode);
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        assert!(
            matches!(
                issue_next(&server, &tokens, "ck_1", NOW),
                Ok(Issued::Ended(_))
            ),
            "{mode_word}"
        );
        assert!(
            !tokens.state().unwrap().grant("ck_1").unwrap().revivable,
            "{mode_word}"
        );
        // the checkout is "approved" whatever it was: still never asked
        mode.store(1, Ordering::SeqCst);
        for day in [8, 30, 91, 400] {
            assert!(
                matches!(
                    issue_next(&server, &tokens, "ck_1", NOW + day * DAY),
                    Ok(Issued::Ended(_))
                ),
                "{mode_word} day {day}"
            );
        }
        assert_eq!(
            asked.load(Ordering::SeqCst),
            1,
            "{mode_word}: never asked again"
        );
    }
}

/// An end is asked again for 90 days and no longer: 89 days on, the restored checkout is signed;
/// 91 days on, nothing is asked.
#[test]
fn x_delta_money3_1_a_payment_taken_back_is_asked_about_for_ninety_days() {
    for (day, signed_again) in [(89, true), (91, false)] {
        let dir = TempDir::new("azcloud-period");
        let tokens = granted(&dir);
        let mode = Arc::new(AtomicUsize::new(0));
        let (fake, asked) = restoring_server(&mode);
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        assert!(matches!(
            issue_next(&server, &tokens, "ck_1", NOW),
            Ok(Issued::Ended(_))
        ));
        mode.store(1, Ordering::SeqCst);
        let answer = issue_next(&server, &tokens, "ck_1", NOW + day * DAY);
        if signed_again {
            assert!(
                matches!(answer, Ok(Issued::Tokens(_))),
                "day {day}: {answer:?}"
            );
        } else {
            assert!(
                matches!(answer, Ok(Issued::Ended(_))),
                "day {day}: {answer:?}"
            );
            assert_eq!(asked.load(Ordering::SeqCst), 1, "day {day}: not asked");
        }
    }
}

/// An answer only a paid checkout gets - here `not_due`, the next month later - clears the end too:
/// the checkout is approved again, and the grant, with its sentence, is alive.
#[test]
fn x_delta_money3_1_a_not_due_answer_proves_the_checkout_is_paid_again_and_clears_the_end() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let mode = Arc::new(AtomicUsize::new(0));
    let (fake, _) = restoring_server(&mode);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Ok(Issued::Ended(_))
    ));
    mode.store(2, Ordering::SeqCst);
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 8 * DAY),
        Ok(Issued::NotDue { .. })
    ));
    let state = tokens.state().unwrap();
    let grant = state.grant("ck_1").unwrap();
    assert!(grant.ended.is_none() && grant.is_open(), "{grant:?}");
    assert_eq!(grant.issued, 1, "the schedule the token server names");
}

/// A final word no restore lifts said to an end that was asked again - the checkout is paid again
/// and every month of it was issued meanwhile to a request this computer does not hold - makes the
/// end final from then on. (A stopped mandate is no such word since X-DELTA-MONEY6-1: it is
/// revivable too, and re-arms the week.)
#[test]
fn x_delta_money3_1_a_revivable_end_that_is_answered_with_another_final_word_becomes_final() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let mode = Arc::new(AtomicUsize::new(0));
    let (fake, asked) = restoring_server(&mode);
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Ok(Issued::Ended(_))
    ));
    mode.store(4, Ordering::SeqCst);
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 8 * DAY),
        Ok(Issued::Ended(_))
    ));
    let grant = tokens.state().unwrap().grant("ck_1").unwrap().clone();
    assert!(grant.is_ended() && !grant.revivable, "{grant:?}");
    assert!(
        grant
            .ended
            .as_deref()
            .is_some_and(|why| why.contains("issued already")),
        "{grant:?}"
    );
    mode.store(1, Ordering::SeqCst);
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 20 * DAY),
        Ok(Issued::Ended(_))
    ));
    assert_eq!(
        asked.load(Ordering::SeqCst),
        2,
        "never asked after the final word"
    );
}

/// Two vaults of one drive joined: the one that lived on - a later look found the checkout paid
/// again and cleared the end - wins against the older end the other holds, in whichever order they
/// are joined; an end after that revival wins against the older revival; an end an older build
/// wrote (no count) never beats a revival. Ordered by the count, not by two computers' clocks.
#[test]
fn x_delta_money3_1_a_revival_wins_the_vault_join_against_an_older_end() {
    let base = VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY);
    let mut ended = base.clone();
    ended.end(String::from("its payment was taken back"), true, NOW);
    let mut revived = ended.clone();
    assert!(revived.revive());
    let joined = |mut kept: VaultGrant, other: &VaultGrant| {
        let mut state = VaultState::with_grant(std::mem::replace(&mut kept, base.clone()));
        state.join(&VaultState::with_grant(other.clone()));
        state.grants.remove(0)
    };
    // an old end held by another device, joined with the revival, in both orders
    assert!(
        joined(ended.clone(), &revived).ended.is_none(),
        "the revival wins joined into the end"
    );
    assert!(
        joined(revived.clone(), &ended).ended.is_none(),
        "and the end does not come back"
    );
    // the checkout reversed again: that end wins against the older revival, in both orders
    let mut again = revived.clone();
    again.end(String::from("taken back again"), true, NOW + DAY);
    assert_eq!(
        joined(revived.clone(), &again).ended.as_deref(),
        Some("taken back again")
    );
    assert_eq!(
        joined(again.clone(), &revived).ended.as_deref(),
        Some("taken back again")
    );
    // an end an older build wrote has no count: it never beats a revival ...
    let mut old = base.clone();
    old.ended = Some(String::from("an older build's end"));
    assert!(joined(revived.clone(), &old).ended.is_none());
    // ... and is taken by a copy that lived (the first end stands, as before)
    assert_eq!(
        joined(base.clone(), &old).ended.as_deref(),
        Some("an older build's end")
    );
    // ... and it is final: this build's look never revives an end that is not a payment taken back
    let mut taken = joined(base.clone(), &old);
    assert!(!taken.revive());
    assert_eq!(taken.ended.as_deref(), Some("an older build's end"));
}

/// The same through a drive's two vaults: this computer's holds the revival, the drive's the older
/// end - the state read from both is revived, and the drive's vault is healed with it.
#[test]
fn x_delta_money3_1_the_drives_vault_holding_the_older_end_is_healed_by_the_revival() {
    use crate::vault::TokenVault;
    let dir = TempDir::new("azcloud-period");
    let store = store_in(&dir);
    let (tokens, in_drive) = super::tokens_in_drive(&store);
    let mut ended = VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY);
    ended.end(String::from("its payment was taken back"), true, NOW);
    // both vaults hold the end; then this computer's look revives it - only its own vault is
    // written while the drive's is not reachable (the other device's copy lags)
    tokens.keep_grant(ended.clone()).unwrap();
    let mut revived = ended.clone();
    revived.revive();
    store
        .local("d_1")
        .unwrap()
        .join(&VaultState::with_grant(revived))
        .unwrap();
    assert!(
        in_drive.read().unwrap().grant("ck_1").unwrap().is_ended(),
        "the drive lags"
    );
    let state = tokens.state().unwrap();
    assert!(
        state.grant("ck_1").unwrap().ended.is_none(),
        "read from both: revived"
    );
    assert!(
        in_drive
            .read()
            .unwrap()
            .grant("ck_1")
            .unwrap()
            .ended
            .is_none(),
        "the drive's vault is healed with the revival"
    );
}

/// The looks reach it: a drive whose vault holds nothing but a grant ended by a payment taken back
/// is looked at when its period is due, the grant asked again - once the checkout is restored it
/// is signed, and the month bought. On the old code the look found nothing held and asked nothing.
#[test]
fn x_delta_money3_1_a_look_asks_a_revivable_end_again_when_a_month_is_due_and_buys_it() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    let mut grant = VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY);
    grant.issued = 1;
    grant.end(String::from("its payment was taken back"), true, NOW);
    tokens.keep_grant(grant).unwrap();
    let keys = keys();
    let issued = AtomicU64::new(1);
    let until = AtomicU64::new(NOW + 9 * DAY);
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
        Ok(json(
            200,
            &serde_json::json!({"period_until": rfc3339(later)}).to_string(),
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // a day after the end: not asked (the week); the drive's period is not due either
    let look = look_at_drive(&server, &shared, &store, &tokens, NOW + DAY);
    assert_eq!(look.redeemed, Redeemed::Nothing, "{:?}", look.redeemed);
    let posts = |fake: &Arc<Fake>| {
        fake.calls()
            .iter()
            .filter(|c| c.url.ends_with("/v1/tokens/issue"))
            .count()
    };
    assert_eq!(posts(&fake), 0);
    // eight days on: the period is due (within a week of its end), the end's week is over: asked,
    // signed, and the month bought
    let look = look_at_drive(&server, &shared, &store, &tokens, NOW + 8 * DAY);
    assert!(
        matches!(look.redeemed, Redeemed::Extended { count: 1, .. }),
        "{:?}",
        look.redeemed
    );
    assert_eq!(posts(&fake), 1);
    let state = tokens.state().unwrap();
    assert!(
        state.grant("ck_1").unwrap().ended.is_none(),
        "the grant lives again"
    );
    assert!(state.tokens.is_empty(), "issued and redeemed");
}

// ==== X-DELTA-MONEY5-8: a request an earlier try may have had counted is not given up ====

/// The token server of the scenario: it hands out `keys`, and answers each issue request by its
/// number `n` (0: the first one) - 0: it signs and COUNTS the request and the answer is lost on the
/// way (the connection drops after the work was done); 1..=3: a proxy that empties the body, so the
/// server answers `bad_request`; from 4 on the proxy is gone: the request it counted is answered
/// again with its signatures (counted once), any other request asks for a month that is counted
/// already (`not_due`, the schedule one month issued). The messages it counted are kept in
/// `counted`; `sent` counts the issue requests.
fn losing_server(
    counted: &Arc<std::sync::Mutex<Option<Vec<String>>>>,
    sent: &Arc<AtomicUsize>,
) -> Arc<Fake> {
    let listed = keys();
    let (counted, sent) = (counted.clone(), sent.clone());
    Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        let n = sent.fetch_add(1, Ordering::SeqCst);
        let messages = blinded_of(call);
        let once = AtomicU64::new(0);
        match n {
            0 => {
                *counted.lock().unwrap() = Some(messages);
                // signed and counted at the token server; nobody hears it
                let _ = signed(call, N1, D1, "100GB/2026", &once);
                Err(String::from("connection reset by peer"))
            }
            1..=3 => Ok(json(
                400,
                r#"{"error": "bad_request", "message": "checkout_id required"}"#,
            )),
            _ if counted.lock().unwrap().as_ref() == Some(&messages) => {
                Ok(signed(call, N1, D1, "100GB/2026", &once))
            }
            _ => Ok(json(
                409,
                &serde_json::json!({"error": "not_due", "message": "later",
                    "schedule": {"months": 3, "issued": 1,
                        "next_issue_at": rfc3339(APPROVED + MONTH)}})
                .to_string(),
            )),
        }
    })
}

/// X-DELTA-MONEY5-8, the finding's own scenario (the fault X-DELTA-MONEY2-10 itself names, a proxy
/// that empties a POST body): the request reaches the token server intact once, is signed and
/// COUNTED, and the answer is lost; the next three looks are mangled to `bad_request`. The client
/// gave the request up on the third word - marked it done, blinded another - and when the proxy
/// recovered the new request was `not_due` (the month is counted) and the first request's signatures,
/// which the server would still replay, were never asked for again: a paid month was gone until the
/// operator edited `tokens_issued`. Now a request one of whose sends ended without an answer may
/// have been counted: it is not given up on any number of `bad_request`s - it stays open, is sent
/// once per look, and a fresh request is blinded beside it - and the day the body arrives intact
/// the replay brings the month. On the old code the last assertion fails: the month never comes.
#[test]
fn x_delta_money5_8_a_request_whose_answer_was_lost_is_not_given_up_on_three_mangled_bodies_and_its_replay_brings_the_month(
) {
    use crate::vault::TokenVault;
    let dir = TempDir::new("azcloud-period");
    let store = store_in(&dir);
    let (tokens, in_drive) = super::tokens_in_drive(&store);
    tokens
        .keep_grant(VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY))
        .unwrap();
    let counted = Arc::new(std::sync::Mutex::new(None));
    let sent = Arc::new(AtomicUsize::new(0));
    let fake = losing_server(&counted, &sent);
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    let posts = || {
        fake.calls()
            .into_iter()
            .filter(|c| c.url.ends_with("/v1/tokens/issue"))
            .collect::<Vec<_>>()
    };

    // look 1: the request is kept, sent, counted at the server - and the answer is lost
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + DAY),
        Err(TokenError::Connect(_))
    ));
    let first = {
        let state = in_drive.read().unwrap();
        let open = state.open_issues("ck_1");
        assert_eq!(open.len(), 1, "the request is kept");
        assert!(
            open[0].may_have_been_counted(),
            "a send without an answer says so, in the drive's vault too"
        );
        open[0].id()
    };

    // looks 2 and 3: the same request again, refused as `bad_request` (the body was emptied)
    for look in 2..=3 {
        assert!(issue_next(&server, &tokens, "ck_1", NOW + look * DAY).is_err());
    }
    assert_eq!(
        blinded_of(&posts()[0]),
        blinded_of(&posts()[2]),
        "the same request again"
    );

    // look 4: its third `bad_request` would have ended it. It may have been counted: it stays open
    // - the done list does not name it - and a fresh request is blinded in the same look
    match issue_next(&server, &tokens, "ck_1", NOW + 4 * DAY) {
        Ok(Issued::NotDue { .. }) => {}
        other => panic!("the fresh request of the month that is counted is not due: {other:?}"),
    }
    let state = tokens.state().unwrap();
    assert!(
        !state.done.contains(&first),
        "the request the token server may have counted is not marked done"
    );
    assert!(
        state.open_issues("ck_1").iter().any(|r| r.id() == first),
        "and is kept for its replay"
    );
    assert_eq!(
        sent.load(Ordering::SeqCst),
        5,
        "once more, then the fresh one"
    );

    // the proxy recovers: the next look sends the kept request once - and the month it counted
    // arrives from its replay
    match issue_next(&server, &tokens, "ck_1", NOW + 5 * DAY) {
        Ok(Issued::Tokens(issued)) => assert_eq!(issued.len(), 1),
        other => panic!("the month the token server counted never came: {other:?}"),
    }
    assert_eq!(sent.load(Ordering::SeqCst), 6, "the replay, once");
    assert!(tokens.state().unwrap().open_issues("ck_1").is_empty());
}

/// The crash row: the process dies after the request was kept and (maybe) sent, before any answer
/// was recorded - nothing in the vault says the send ended without an answer. A request found open
/// at the start of a look with no outcome recorded for it (no refusal counted, no mark) is one
/// whose earlier try is unaccounted for: the look marks it before it sends it again, so the same
/// three `bad_request`s do not end it.
#[test]
fn x_delta_money5_8_a_request_found_open_with_no_outcome_recorded_is_not_given_up_either() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let listed = keys();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        Ok(json(
            400,
            r#"{"error": "bad_request", "message": "checkout_id required"}"#,
        ))
    });
    let transport = Shared(fake.clone());
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // a request kept by a look that was cut right after it, with nothing recorded of its send
    let key = crate::period::IssuerKey {
        tier: String::from("100GB"),
        year: 2026,
        key_id: String::from("100GB/2026"),
        public_key_pem: pem(N1),
    };
    let cut = crate::period::IssueRequest::new(&key, "ck_1", 1).unwrap();
    assert!(!cut.may_have_been_counted(), "a request as it is made");
    tokens
        .join(&VaultState {
            issues: vec![cut.clone()],
            ..VaultState::default()
        })
        .unwrap();

    for look in 1..=4 {
        let _ = issue_next(&server, &tokens, "ck_1", NOW + look * DAY);
    }
    let state = tokens.state().unwrap();
    assert!(
        !state.done.contains(&cut.id()),
        "four looks of `bad_request`: not given up"
    );
    let open = state.open_issues("ck_1");
    assert!(
        open.iter()
            .any(|r| r.id() == cut.id() && r.may_have_been_counted()),
        "marked at the first look that found it: {open:?}"
    );
}

/// The negative control: a request every send of which was ANSWERED - refused as `bad_request` each
/// time - was never counted, and is given up on the third word as X-DELTA-MONEY2-10 has it (the
/// request it made first and sent in one look is not "found open", and no send of it went
/// unanswered).
#[test]
fn x_delta_money5_8_a_request_every_send_of_which_was_answered_is_still_given_up() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let listed = keys();
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        Ok(json(
            400,
            r#"{"error": "bad_request", "message": "blind sign: no"}"#,
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    for look in 1..=3 {
        let _ = issue_next(&server, &tokens, "ck_1", NOW + look * DAY);
    }
    let state = tokens.state().unwrap();
    assert_eq!(
        state.done.len(),
        1,
        "the request every send of which was refused is given up"
    );
    assert!(
        state
            .open_issues("ck_1")
            .iter()
            .all(|r| !r.may_have_been_counted()),
        "and the new one has no history"
    );
}

/// Which failures of a send may have reached the token server and been counted: every one but those
/// that sent nothing (a request that is not built) and a busy server that says it did nothing.
#[test]
fn x_delta_money5_8_only_a_failure_that_sent_nothing_or_did_nothing_is_no_doubt() {
    use crate::{answer::Spent, period::send_may_have_been_counted};
    let busy = |spent: Spent| TokenError::Busy {
        status: 503,
        code: String::from("busy"),
        message: String::new(),
        retry_after: None,
        spent,
    };
    assert!(!send_may_have_been_counted(&TokenError::Config(
        String::from("no issue key")
    )));
    assert!(!send_may_have_been_counted(&busy(Spent::Nothing)));
    assert!(send_may_have_been_counted(&busy(Spent::Maybe)));
    assert!(send_may_have_been_counted(&TokenError::Connect(
        String::from("reset")
    )));
    assert!(send_may_have_been_counted(&TokenError::Protocol(
        String::from("does not read")
    )));
    assert!(send_may_have_been_counted(&TokenError::Refused {
        status: 403,
        code: String::from("proxy"),
        message: String::new(),
    }));
    assert!(send_may_have_been_counted(&TokenError::SignIn(
        String::from("401")
    )));
}

/// Rule 15: the mark is additive in the kept request's JSON and joins as an OR in every vault: a
/// request never refused or lost writes no mark, a request from before it reads without one, and a
/// copy that knows the send was lost makes the joined request say so.
#[test]
fn x_delta_money5_8_the_mark_is_additive_and_joins_as_an_or() {
    let key = crate::period::IssuerKey {
        tier: String::from("100GB"),
        year: 2026,
        key_id: String::from("100GB/2026"),
        public_key_pem: pem(N1),
    };
    let plain = crate::period::IssueRequest::new(&key, "ck_1", 1).unwrap();
    let text = serde_json::to_string(&plain).unwrap();
    assert!(!text.contains("unanswered"), "{text}");
    let old: crate::period::IssueRequest = serde_json::from_str(&text).unwrap();
    assert!(!old.may_have_been_counted());
    let lost = plain.with_unanswered_send();
    let marked = serde_json::to_string(&lost).unwrap();
    assert!(marked.contains(r#""unanswered":true"#), "{marked}");
    assert!(serde_json::from_str::<crate::period::IssueRequest>(&marked)
        .unwrap()
        .may_have_been_counted());
    // two vaults, one that knows and one that does not: joined either way, the request is marked
    for (first, second) in [(&plain, &lost), (&lost, &plain)] {
        let mut a = VaultState {
            issues: vec![first.clone()],
            ..VaultState::default()
        };
        a.join(&VaultState {
            issues: vec![second.clone()],
            ..VaultState::default()
        });
        assert_eq!(a.issues.len(), 1);
        assert!(a.issues[0].may_have_been_counted());
    }
}

// ==== X-DELTA-MONEY6-1: a mandate stopped for a reversal the restore lifts ====

/// The token server of `ck_1` and `ck_2`: `keys`, and each issue request answered by the word its
/// checkout stands at - `stop` (the mandate a repeat reversal stopped: `mandate_stopped`), `back`
/// (the payment was taken back: `not_paid`) or paid (it signs). `ck_1` is paid when `paid.0` is
/// set, `ck_2` when `paid.1` is. `asked` counts the issue requests.
fn stopped_server(
    word_of: [&'static str; 2],
    paid: &Arc<[std::sync::atomic::AtomicBool; 2]>,
    asked: &Arc<AtomicUsize>,
) -> Arc<Fake> {
    let listed = keys();
    let (paid, asked) = (paid.clone(), asked.clone());
    let issued = AtomicU64::new(0);
    Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        asked.fetch_add(1, Ordering::SeqCst);
        let body: serde_json::Value = serde_json::from_slice(&call.body).unwrap();
        let at = usize::from(body["checkout_id"] == "ck_2");
        if paid[at].load(Ordering::SeqCst) {
            return Ok(signed_for(
                call,
                body["checkout_id"].as_str().unwrap(),
                N1,
                D1,
                "100GB/2026",
                &issued,
            ));
        }
        Ok(match word_of[at] {
            "stop" => json(409, r#"{"error": "mandate_stopped", "message": "stopped"}"#),
            _ => json(
                409,
                r#"{"error": "not_paid", "message": "the checkout is not approved"}"#,
            ),
        })
    })
}

fn nobody_paid() -> Arc<[std::sync::atomic::AtomicBool; 2]> {
    Arc::new([
        std::sync::atomic::AtomicBool::new(false),
        std::sync::atomic::AtomicBool::new(false),
    ])
}

/// X-DELTA-MONEY6-1, the finding's scenario: a payer's second reversal stops the mandate, every issue
/// on it is answered `mandate_stopped`, an app asks inside that window - the grant ended, and the
/// old client had it end for good: `IssueRefusal::is_revivable` named `not_paid` only. Then the
/// operator's restore (or the refund that failed at the provider) lifts the stop, and the months
/// were never asked for again. Now the end is a revivable one: the looks of the week ask nothing, a
/// week on the ask is signed, the grant lives, the sentence goes and the month is issued. On the old
/// code the first assertion on the grant fails (`revivable` is false) and the month never comes.
#[test]
fn x_delta_money6_1_a_grant_ended_by_a_stopped_mandate_issues_its_month_after_the_restore() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let paid = nobody_paid();
    let asked = Arc::new(AtomicUsize::new(0));
    let transport = Shared(stopped_server(["stop", "stop"], &paid, &asked));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // the look that learns the stop ends the grant and says why
    match issue_next(&server, &tokens, "ck_1", NOW) {
        Ok(Issued::Ended(why)) => assert!(why.contains("mandate was stopped"), "{why}"),
        other => panic!("not ended: {other:?}"),
    }
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    let state = tokens.state().unwrap();
    let grant = state.grant("ck_1").unwrap();
    assert!(
        grant.is_ended() && grant.revivable,
        "a restore lifts the stop: the end is revivable: {grant:?}"
    );
    assert!(!grant.is_open());
    // the restore comes; the looks of the week ask nothing
    paid[0].store(true, Ordering::SeqCst);
    for day in [1, 3, 6] {
        assert!(
            matches!(
                issue_next(&server, &tokens, "ck_1", NOW + day * DAY),
                Ok(Issued::Ended(_))
            ),
            "day {day}"
        );
    }
    assert_eq!(asked.load(Ordering::SeqCst), 1, "not asked within the week");
    // a week on: asked, signed - the month comes, the grant lives, its sentence is gone
    match issue_next(&server, &tokens, "ck_1", NOW + 8 * DAY) {
        Ok(Issued::Tokens(issued)) => assert_eq!(issued.len(), 1),
        other => panic!("the month did not come: {other:?}"),
    }
    let state = tokens.state().unwrap();
    let grant = state.grant("ck_1").unwrap();
    assert!(
        grant.ended.is_none() && !grant.revivable && grant.is_open(),
        "{grant:?}"
    );
    assert_eq!(grant.issued, 1);
    assert_eq!(state.tokens.len(), 1, "the month's token is kept");
    assert_eq!(asked.load(Ordering::SeqCst), 2);
}

/// X-DELTA-MONEY6-1, the stop that stands: asked a week apart for 90 days, refused each time (the
/// next ask is counted from the last one), and then - nothing is asked any more, a restore a day
/// after the 90 days included: the grant ends for good with its sentence. With a look every day the
/// asks fall on days 7, 14, ..., 84 (the 12th week ends on day 84; day 91 is past the 90 days).
#[test]
fn x_delta_money6_1_a_stop_that_stands_for_ninety_days_ends_the_grant_for_good_with_its_sentence() {
    let dir = TempDir::new("azcloud-period");
    let tokens = granted(&dir);
    let paid = nobody_paid();
    let asked = Arc::new(AtomicUsize::new(0));
    let transport = Shared(stopped_server(["stop", "stop"], &paid, &asked));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Ok(Issued::Ended(_))
    ));
    // a look a day, the stop stands: asked on days 7, 14, ..., 84 and on no day after
    let mut last_ask_day = 0;
    for day in 1..=95_u64 {
        let before = asked.load(Ordering::SeqCst);
        assert!(
            matches!(
                issue_next(&server, &tokens, "ck_1", NOW + day * DAY),
                Ok(Issued::Ended(_))
            ),
            "day {day}"
        );
        if asked.load(Ordering::SeqCst) > before {
            last_ask_day = day;
            assert!(
                day < 90 && day % 7 == 0,
                "asked on day {day}: a week after the last ask, never from the 90th day on"
            );
        }
    }
    assert_eq!(last_ask_day, 84, "the last ask of the 90 days");
    assert_eq!(
        asked.load(Ordering::SeqCst),
        13,
        "the first ask at the end, then one a week"
    );
    // the restore a day after the 90 days is too late: the grant is ended for good, sentence and all
    paid[0].store(true, Ordering::SeqCst);
    match issue_next(&server, &tokens, "ck_1", NOW + 91 * DAY) {
        Ok(Issued::Ended(why)) => assert!(why.contains("mandate was stopped"), "{why}"),
        other => panic!("{other:?}"),
    }
    assert_eq!(
        asked.load(Ordering::SeqCst),
        13,
        "never asked after the 90 days"
    );
    let grant = tokens.state().unwrap().grant("ck_1").unwrap().clone();
    assert!(
        grant.is_ended() && !grant.revive_pending(NOW + 91 * DAY),
        "{grant:?}"
    );
}

/// X-DELTA-MONEY6-1 (e): a grant ended by `not_paid` and one ended by `mandate_stopped`, in one
/// drive's vault, are each its own record. The restore lifts the mandate's stop and not the other's
/// reversal: the first ask of the week revives the one grant, refuses the other and re-arms ITS week,
/// and neither end's sentence, kind or day is written over by the other's.
#[test]
fn x_delta_money6_1_an_end_by_not_paid_and_an_end_by_a_stopped_mandate_keep_their_own_records() {
    let dir = TempDir::new("azcloud-period");
    let tokens = store_in(&dir).on_this_device("d_1").unwrap();
    for id in ["ck_1", "ck_2"] {
        tokens
            .keep_grant(VaultGrant::new(id, "100GB", 3, ISSUE_KEY))
            .unwrap();
    }
    let paid = nobody_paid();
    let asked = Arc::new(AtomicUsize::new(0));
    // ck_1: the payment was taken back; ck_2: the mandate was stopped for a reversal
    let transport = Shared(stopped_server(["back", "stop"], &paid, &asked));
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    for id in ["ck_1", "ck_2"] {
        assert!(
            matches!(issue_next(&server, &tokens, id, NOW), Ok(Issued::Ended(_))),
            "{id}"
        );
    }
    let state = tokens.state().unwrap();
    let (first, second) = (state.grant("ck_1").unwrap(), state.grant("ck_2").unwrap());
    assert!(first.revivable && second.revivable);
    assert!(
        first
            .ended
            .as_deref()
            .is_some_and(|w| w.contains("taken back")),
        "{first:?}"
    );
    assert!(
        second
            .ended
            .as_deref()
            .is_some_and(|w| w.contains("mandate was stopped")),
        "{second:?}"
    );
    assert_eq!((first.ended_at, second.ended_at), (Some(NOW), Some(NOW)));
    // the restore lifts the stop of ck_2 only
    paid[1].store(true, Ordering::SeqCst);
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW + 8 * DAY),
        Ok(Issued::Ended(_))
    ));
    assert!(matches!(
        issue_next(&server, &tokens, "ck_2", NOW + 8 * DAY),
        Ok(Issued::Tokens(_))
    ));
    let state = tokens.state().unwrap();
    let (first, second) = (state.grant("ck_1").unwrap(), state.grant("ck_2").unwrap());
    assert!(
        second.ended.is_none() && second.is_open(),
        "revived: {second:?}"
    );
    assert!(
        first.is_ended() && first.revivable && first.ended_at == Some(NOW),
        "ck_1 keeps its own end: {first:?}"
    );
    assert!(
        first
            .ended
            .as_deref()
            .is_some_and(|w| w.contains("taken back")),
        "{first:?}"
    );
    assert_eq!(
        first.next_issue_at,
        Some(NOW + 8 * DAY + 7 * DAY),
        "its week is counted from its own ask"
    );
    // and the second vault of the drive agrees, joined in either order
    let mut joined = VaultState::default();
    joined.join(&state);
    assert!(
        joined.grant("ck_1").unwrap().is_ended() && joined.grant("ck_2").unwrap().ended.is_none()
    );
}

/// X-DELTA-MONEY6-1, the looks: a drive whose vault holds a grant that a stopped mandate ended is
/// looked at when its period is due and the end's week is over - and once the stop is lifted the
/// month is signed and bought. The end is learned by the production path (`issue_next` at the
/// claim), not planted: on the old code the grant is final, the look finds nothing held and asks
/// nothing, and the first assertion after the restore fails.
#[test]
fn x_delta_money6_1_a_look_asks_a_stopped_mandate_again_and_buys_the_month_after_the_restore() {
    let dir = TempDir::new("azcloud-period");
    let shared = keyring_with_session(&dir, "dt_f.3.newest");
    let store = store_in(&dir);
    let tokens = store.on_this_device("d_1").unwrap();
    tokens
        .keep_grant(VaultGrant::new("ck_1", "100GB", 3, ISSUE_KEY))
        .unwrap();
    let paid = nobody_paid();
    let asked = Arc::new(AtomicUsize::new(0));
    let (listed, issued, until) = (keys(), AtomicU64::new(0), AtomicU64::new(NOW + 9 * DAY));
    let (restored, counted) = (paid.clone(), asked.clone());
    let fake = Fake::new(move |call, _| {
        if call.url.ends_with("/v1/tokens/keys") {
            return Ok(json(200, &listed));
        }
        if call.url.ends_with("/v1/tokens/issue") {
            counted.fetch_add(1, Ordering::SeqCst);
            return Ok(if restored[0].load(Ordering::SeqCst) {
                signed(call, N1, D1, "100GB/2026", &issued)
            } else {
                json(409, r#"{"error": "mandate_stopped", "message": "stopped"}"#)
            });
        }
        if call.method == Method::Get {
            let answer = serde_json::json!({
                "id": "d_1", "tier": "100GB", "period_until": rfc3339(until.load(Ordering::SeqCst)),
            });
            return Ok(json(200, &answer.to_string()));
        }
        assert_eq!(call.url, format!("{TOKEN}/v1/drives/d_1/redeem"));
        let later = until.fetch_add(30 * DAY, Ordering::SeqCst) + 30 * DAY;
        Ok(json(
            200,
            &serde_json::json!({"period_until": rfc3339(later)}).to_string(),
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    // the claim's first month is asked while the stop stands
    assert!(matches!(
        issue_next(&server, &tokens, "ck_1", NOW),
        Ok(Issued::Ended(_))
    ));
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    // a look a day on: nothing is due, nothing asked
    let look = look_at_drive(&server, &shared, &store, &tokens, NOW + DAY);
    assert_eq!(look.redeemed, Redeemed::Nothing, "{:?}", look.redeemed);
    assert_eq!(asked.load(Ordering::SeqCst), 1);
    // the restore; eight days on the period is due, the end's week is over: asked, signed, bought
    paid[0].store(true, Ordering::SeqCst);
    let look = look_at_drive(&server, &shared, &store, &tokens, NOW + 8 * DAY);
    assert!(
        matches!(look.redeemed, Redeemed::Extended { count: 1, .. }),
        "{:?}",
        look.redeemed
    );
    assert_eq!(asked.load(Ordering::SeqCst), 2);
    let state = tokens.state().unwrap();
    assert!(
        state.grant("ck_1").unwrap().ended.is_none(),
        "the grant lives again"
    );
    assert!(state.tokens.is_empty(), "issued and redeemed");
}

