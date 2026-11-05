//! Gate: money in the Azlin apps is typed, a checkout leaves the keyring's list only with
//! evidence, and a refusal is decided by its type (ARCH28 Z-18; RF51, RF52).
//!
//! The gate reads the shipped code of the contract, the kit, the checkout's client half, the
//! payment methods' data and AzDrive (comments and test items blanked, as every gate of the
//! apps reads: `azul-srcgate`) and fails when
//!
//! 1. `Settled(` or `SettledBy` stands outside `pending/proof.rs`: a checkout's claim secret and
//!    issue key are erased only on a proof made there from evidence nobody else constructs;
//! 2. a file that names `TokenError` decides by `status: 4xx`, `status ==`, `code == "word"`,
//!    `.contains(&code.as_str())`, or holds one of the contract's code words as a literal,
//!    outside the two files that decode a refusal (`token/refusal.rs`, `answer.rs`);
//! 3. an amount (`amount_cents: u64`) or a currency (`currency: String`) stands alone outside
//!    the wire and the records of the wire;
//! 4. an amount, VAT or currency is defaulted: `.unwrap_or("EUR")`, `.unwrap_or(0)` and
//!    `.unwrap_or_default()` in a statement that talks of one;
//! 5. `l10n::money(` is called anywhere but `pay_words::show`.
//!
//! The allowances name the file, the rule, how many and why. A hit no allowance takes fails, an
//! allowance whose count is not the code's fails and so does one nothing uses any more: the
//! counts only go down, and a number that went down is written down. The rules' planted and
//! clean cases run first ([`azul_srcgate::self_test`]), so a rule that finds nothing cannot pass
//! for a clean tree. The rules are in `rules.rs`.

mod rules;

use azlin_contract::{CheckoutRefusal, ClaimRefusal, IssueRefusal, RedeemRefusal};
use rules::{default_gate, pair_gate, refusal_gate, settled_gate, show_gate};

/// The issue refusals' words that no other refusal shares with a meaning of its own
/// (`bad_request`, `try_again`, `unavailable` are generic: `answer.rs` decides them).
const ISSUE_WORDS: &[&str] = &[
    "issue_key_required",
    "key_id_required",
    "issue_key_wrong",
    "no_such_checkout",
    "not_paid",
    "mandate_stopped",
    "already_issued",
    "not_due",
    "key_changed",
    "issuer_key_missing",
];

/// The same for the redemption's.
const REDEEM_WORDS: &[&str] = &[
    "wrong_tier",
    "token_expired",
    "unknown_issuer",
    "bad_token",
    "token_used",
    "change_not_due",
    "does_not_fit",
    "stored_bytes_required",
    "archive_proof_required",
];

/// The contract's code words of a checkout, a claim, an issue and a redemption - the ones whose
/// refusal the kit types ([`azcloud_kit::token::Refusal`]).
fn contract_words() -> Vec<String> {
    let mut words: Vec<String> = Vec::new();
    for refusal in CheckoutRefusal::ALL {
        if refusal != CheckoutRefusal::Unknown {
            words.push(refusal.as_str().to_string());
        }
    }
    words.extend(ClaimRefusal::ALL.iter().map(|r| r.as_str().to_string()));
    words.extend(ISSUE_WORDS.iter().map(|w| (*w).to_string()));
    words.extend(REDEEM_WORDS.iter().map(|w| (*w).to_string()));
    words.sort();
    words.dedup();
    words
}

#[test]
fn a_checkout_leaves_the_list_only_with_a_settled_made_in_proof_rs() {
    settled_gate().run();
}

#[test]
fn a_refusal_is_decided_by_its_type_and_not_by_a_status_number_or_a_word() {
    refusal_gate(&contract_words()).run();
}

#[test]
fn an_amount_and_a_currency_stand_alone_only_on_the_wire_and_in_its_records() {
    pair_gate().run();
}

#[test]
fn no_amount_vat_or_currency_is_defaulted() {
    default_gate().run();
}

#[test]
fn money_is_written_for_a_person_by_one_function() {
    show_gate().run();
}

/// The hand-listed words are words of the contract: one that is not any more (renamed, removed)
/// would leave the word rule looking for a spelling nobody sends.
#[test]
fn the_listed_issue_and_redeem_words_are_still_the_contracts() {
    for word in ISSUE_WORDS {
        let parsed = serde_json::from_str::<IssueRefusal>(&format!(r#"{{"error": "{word}"}}"#));
        assert!(
            !matches!(parsed, Ok(IssueRefusal::Unknown)),
            "{word} is no word of IssueRefusal any more: update ISSUE_WORDS"
        );
    }
    for word in REDEEM_WORDS {
        let parsed = serde_json::from_str::<RedeemRefusal>(&format!(r#"{{"error": "{word}"}}"#));
        assert!(
            !matches!(parsed, Ok(RedeemRefusal::Unknown)),
            "{word} is no word of RedeemRefusal any more: update REDEEM_WORDS"
        );
    }
    // The words the gate looks for include every checkout word: a new one is looked for at once.
    let words = contract_words();
    for refusal in CheckoutRefusal::ALL {
        if refusal != CheckoutRefusal::Unknown {
            assert!(
                words.iter().any(|w| w == refusal.as_str()),
                "{}",
                refusal.as_str()
            );
        }
    }
}
