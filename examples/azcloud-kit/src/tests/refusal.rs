//! RF51 / AR5b-6: a refusal of the token server is read ONCE, as the contract's typed value
//! (`token/refusal.rs`), and the decisions of the money path match that: a word the contract
//! names, never a status number.

use azlin_contract::{ArchiveRefusal, AzlinError, CheckoutRefusal, ClaimRefusal, RedeemRefusal};

use super::{json, Fake, Shared, TOKEN};
use crate::{
    token::{CheckoutStatus, Refusal, TokenError, TokenServer},
    ClaimKey,
};

fn refused(status: u16, code: &str) -> TokenError {
    TokenError::Refused {
        status,
        code: code.to_string(),
        message: String::new(),
    }
}

/// X-AR5b-5: every word of the contract's families reads as its typed value, a word it does not
/// name (and an answer with none) is `Unclassified` with the status it came with.
#[test]
fn x_ar5b_5_a_word_of_the_contract_reads_as_its_typed_refusal() {
    // Every checkout word, with its own status, is a checkout word (or one both services
    // share): never Unclassified, never another family's.
    for refusal in CheckoutRefusal::ALL {
        if refusal == CheckoutRefusal::Unknown {
            continue;
        }
        let read = Refusal::decode(refusal.status(), refusal.as_str());
        assert!(
            matches!(read, Refusal::Checkout(_) | Refusal::Service(_)),
            "{}: {read:?}",
            refusal.as_str()
        );
    }
    assert_eq!(
        Refusal::decode(404, "no_such_checkout"),
        Refusal::Checkout(CheckoutRefusal::NoSuchCheckout)
    );
    assert_eq!(
        Refusal::decode(403, " blocked_country "),
        Refusal::Checkout(CheckoutRefusal::BlockedCountry)
    );
    for claim in ClaimRefusal::ALL {
        assert_eq!(
            Refusal::decode(claim.status(), claim.as_str()),
            Refusal::Claim(claim)
        );
    }
    assert_eq!(
        Refusal::decode(409, "last_recovery_key"),
        Refusal::Service(AzlinError::LastRecoveryKey)
    );
    assert_eq!(
        Refusal::decode(409, "token_used"),
        Refusal::Redeem(RedeemRefusal::TokenUsed)
    );
    assert_eq!(
        Refusal::decode(401, "bad_signature"),
        Refusal::Archive(ArchiveRefusal::BadSignature)
    );
    // What the contract does not name stays what it was.
    for (status, code) in [
        (404, ""),
        (404, "not_found"),
        (400, "voucher_too_small"),
        (403, "forbidden"),
    ] {
        assert_eq!(
            Refusal::decode(status, code),
            Refusal::Unclassified {
                status,
                code: code.to_string()
            },
            "{status} {code:?}"
        );
    }
    // The catch-all words are no words: a newer server's "unknown" is not a refusal this build
    // can act on.
    assert!(matches!(
        Refusal::decode(500, "unknown"),
        Refusal::Unclassified { .. }
    ));
}

/// The questions the money path asks, answered by the typed value and by nothing else.
#[test]
fn x_ar5b_5_the_questions_of_the_money_path_are_answered_by_the_typed_value() {
    assert!(refused(404, "no_such_checkout").is_no_such_checkout());
    assert!(!refused(404, "not_found").is_no_such_checkout());
    assert!(!refused(404, "").is_no_such_checkout());
    assert!(refused(404, "not_found").is_route_missing());
    assert!(refused(404, "").is_route_missing());
    assert!(!refused(404, "no_such_checkout").is_route_missing());
    assert!(refused(404, "not_found").is_checkout_only());
    assert!(refused(403, "blocked_country").is_blocked_country());
    assert!(!refused(403, "forbidden").is_blocked_country());
    assert!(!refused(403, "").is_blocked_country());
    for word in [
        "lock_used",
        "lock_key_wrong",
        "lock_tier",
        "no_lock",
        "not_paid",
    ] {
        assert!(refused(409, word).is_lock_refused(), "{word}");
    }
    assert!(!refused(409, "checkout_paid").is_lock_refused());
    assert!(refused(409, "checkout_paid").is_checkout_paid());
    for word in ["token_used", "bad_token", "token_expired", "unknown_issuer"] {
        assert!(refused(400, word).is_useless_token(), "{word}");
    }
    assert!(!refused(409, "change_not_due").is_useless_token());
    assert!(!refused(400, "wrong_tier").is_useless_token());
    assert!(refused(400, "unknown_issuer").is_doubtful_token());
    assert!(!refused(409, "token_used").is_doubtful_token());
    assert_eq!(
        refused(409, "claims_used").claim_refusal(),
        Some(ClaimRefusal::ClaimsUsed)
    );
    assert_eq!(
        refused(410, "claim_expired").claim_refusal(),
        Some(ClaimRefusal::ClaimExpired)
    );
    assert_eq!(refused(409, "checkout_paid").claim_refusal(), None);
    assert!(refused(401, "unauthorized").is_unauthorized());
    // An error without an answer has no refusal.
    assert_eq!(TokenError::Connect(String::from("down")).refusal(), None);
    assert_eq!(TokenError::SignIn(String::from("gone")).refusal(), None);
    // A busy server's refusal reads the same.
    let busy = TokenError::Busy {
        status: 503,
        code: String::from("no_price_ladder"),
        message: String::new(),
        retry_after: None,
        spent: crate::answer::Spent::Nothing,
    };
    assert_eq!(
        busy.refusal(),
        Some(Refusal::Checkout(CheckoutRefusal::NoPriceLadder))
    );
    // Its words for a sentence: its own sentence, else its word.
    assert_eq!(refused(409, "checkout_paid").said(), "checkout_paid");
    let with_sentence = TokenError::Refused {
        status: 404,
        code: String::from("no_such_checkout"),
        message: String::from("unknown checkout"),
    };
    assert_eq!(with_sentence.said(), "unknown checkout");
}

/// AZ-P1: a poll answered 404 is the end of the checkout only in the contract's word
/// `no_such_checkout`. A bare `not_found` (a route missing in a rollback), a 404 with no JSON at
/// all (a proxy's or an edge's page) and a 404 of another word leave it as it is: it may be
/// paid, its claim secret is the only key to the drive.
#[test]
fn az_p1_a_poll_answered_404_without_the_contracts_word_is_not_a_gone_checkout() {
    let claim = ClaimKey::generate().unwrap();
    for (name, reply) in [
        (
            "a bare not_found",
            json(404, r#"{"error": "not_found", "message": "no route"}"#),
        ),
        ("an empty JSON", json(404, "{}")),
        (
            "an HTML page",
            azul_storage::HttpReply {
                status: 404,
                headers: vec![(String::from("content-type"), String::from("text/html"))],
                body: b"<html><body>404 Not Found</body></html>".to_vec(),
            },
        ),
        ("another word", json(404, r#"{"error": "tier_gone"}"#)),
    ] {
        let fake = Fake::new(move |_, _| Ok(reply.clone()));
        let transport = Shared(fake);
        let server = TokenServer::new(TOKEN, &transport).unwrap();
        match server.checkout_status("ck_1", &claim) {
            Err(TokenError::Refused { status: 404, .. }) => {}
            other => {
                panic!("{name}: the checkout is not gone, it is not known what it is: {other:?}")
            }
        }
    }
    let fake = Fake::new(|_, _| {
        Ok(json(
            404,
            r#"{"error": "no_such_checkout", "message": "unknown checkout"}"#,
        ))
    });
    let transport = Shared(fake);
    let server = TokenServer::new(TOKEN, &transport).unwrap();
    assert_eq!(
        server.checkout_status("ck_1", &claim).unwrap(),
        CheckoutStatus::Gone(String::from("unknown checkout")),
        "the server's own word is the end"
    );
}
