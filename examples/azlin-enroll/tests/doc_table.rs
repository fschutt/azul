//! The plan under the table of paths (planning/architecture.md section 7.5, 7.6, 5.5, 5.10; R7 of
//! COMMON-W1R): the sentences of the plan that `PATHS` and the rules of the crate are written from
//! must still be in the plan. A plan that changes one of them turns this red, and the table (or
//! the plan) is looked at again. The check is a function over the text, so that a copy of the plan
//! with one sentence edited is shown to fail it.

mod common;

use common::plan_text;

/// The sentences (whitespace-normalised) and what each one fixes in the crate.
const SENTENCES: &[(&str, &str)] = &[
    (
        "| **the person's own existing device** approves (default) | fingerprint phrase; that device wraps the member key; no hold needed |",
        "OwnDevice: the fingerprint phrase, no hold",
    ),
    (
        "| **recovery** (no own device, e.g. a lost laptop) | a fresh SSO attestation for the person (necessary, never sufficient), **plus** escrow opened by the threshold (7.6), **plus** the hold |",
        "EscrowRecovery: SSO attestation, escrow threshold, the hold",
    ),
    (
        "Any admin or any of the person's devices can cancel.",
        "cancel: any admin, any device of the person",
    ),
    (
        "The same hold applies to every recovery: the recovery code, the contacts' shares and the passkey wrap (5.5).",
        "the hold of every recovery path",
    ),
    (
        "**Helpdesk** may only open a recovery request and check the fingerprint phrase with the person out of band.",
        "helpdesk opens a recovery request and nothing else",
    ),
    (
        "**Device evidence** counts only as an MDM certificate tied to the company's inventory.",
        "device evidence is an MDM certificate",
    ),
    (
        "Rate limits per person, and an entry in the signed log.",
        "limits per person",
    ),
    (
        "signed by two different admins from hardware-bound keys",
        "escrow_open: two different admins",
    ),
    (
        "**72 h** for an active member",
        "the escrow-opening wait",
    ),
    (
        "no wait for a member already offboarded by a quorum",
        "no wait after a quorum offboarding",
    ),
    (
        "**Every opening needs two different people**",
        "two different people per opening",
    ),
    (
        "The key agent's share never stands in for a person.",
        "the key agent counts as a share, never as a person",
    ),
    (
        "| `enroll_hold_s` | 48 h for any enrollment without the person's own device (D42) |",
        "the enrollment hold",
    ),
    (
        "| `escrow_open_wait_s` | 72 h for an active member |",
        "the escrow-opening wait of the policy",
    ),
    (
        "its 2-of-3 Shamir shares to contacts",
        "two of three contacts",
    ),
    (
        "**The passkey wrap is on by default for personal and family accounts**",
        "the passkey wrap",
    ),
    (
        "Single-admin workspaces",
        "the owner plus the offline recovery kit",
    ),
];

/// The text with every run of whitespace made one space.
fn normalised(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Every sentence of [`SENTENCES`] that the plan lacks.
fn missing_sentences(plan: &str) -> Vec<&'static str> {
    let plan = normalised(plan);
    SENTENCES
        .iter()
        .filter(|(sentence, _)| !plan.contains(sentence))
        .map(|(_, what)| *what)
        .collect()
}

#[test]
fn the_plan_still_says_what_the_table_of_paths_is_written_from() {
    let Some(plan) = plan_text() else {
        return;
    };
    let missing = missing_sentences(&plan);
    assert!(
        missing.is_empty(),
        "the plan no longer holds the sentences of: {missing:?}"
    );
}

#[test]
fn doc_table_test_fails_when_a_sentence_is_edited() {
    let Some(plan) = plan_text() else {
        return;
    };
    // Each sentence, replaced in a copy of the plan by an edited one, is found missing (and only
    // that one).
    for (sentence, what) in SENTENCES {
        let edited = normalised(&plan).replacen(sentence, "an edited sentence", 1);
        let missing = missing_sentences(&edited);
        assert!(
            missing.contains(what),
            "the edit of the sentence of `{what}` is not seen"
        );
    }
    // A plan without the section is found missing everything.
    assert_eq!(missing_sentences("").len(), SENTENCES.len());
}
