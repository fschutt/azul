//! Check R16 of lane W1-S46a: decisions validate against their report (planning/architecture.md section 3.9:
//! "no action, block the link (`link_blocks`, every node at once), lock the account (read only), ban, legal hold,
//! or refer to the authorities (Art. 18 ...). Each decision has a reason from a fixed list plus text").
//! Every test enters through the public API of the crate.

mod common;

use azlin_reports::{Action, Decision, DecisionError, Field, Reason, ReportBody, Text};

use common::*;

/// The five kinds of subject a decision can meet (a `File` with and without a link differ).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    Link,
    Chat,
    Member,
    FileWithLink,
    FileWithoutLink,
}

const KINDS: [Kind; 5] = [
    Kind::Link,
    Kind::Chat,
    Kind::Member,
    Kind::FileWithLink,
    Kind::FileWithoutLink,
];

fn body_of(kind: Kind, reason: Reason) -> ReportBody {
    match kind {
        Kind::Link => link_body(reason),
        Kind::Chat => chat_body(reason, vec![chat_record(1)]),
        Kind::Member => member_body(reason),
        Kind::FileWithLink => file_body(reason, true, "a file"),
        Kind::FileWithoutLink => file_body(reason, false, "a file"),
    }
}

fn decision(report: &ReportBody, action: Action, reason: Reason, with_text: bool) -> Decision {
    Decision {
        report: report.id,
        action,
        reason,
        text: if with_text {
            Some(text("the operator's words"))
        } else {
            None
        },
        day: day(100),
    }
}

/// What a decision needs of its SUBJECT, written out as a table: rows are the actions, columns the kinds in
/// the order of `KINDS` (Link, Chat, Member, FileWithLink, FileWithoutLink). `None` = fine.
const NEEDS_LINK: Option<DecisionError> = Some(DecisionError::NeedsLink);
const NEEDS_ACCOUNT: Option<DecisionError> = Some(DecisionError::NeedsAccount);
const SUBJECT_TABLE: [(Action, [Option<DecisionError>; 5]); 6] = [
    (Action::NoAction, [None, None, None, None, None]),
    (
        Action::BlockLink,
        [None, NEEDS_LINK, NEEDS_LINK, None, NEEDS_LINK],
    ),
    (Action::LockAccount, [None, None, None, None, NEEDS_ACCOUNT]),
    (Action::Ban, [None, None, None, None, NEEDS_ACCOUNT]),
    (Action::LegalHold, [None, None, None, None, NEEDS_ACCOUNT]),
    (Action::ReferToAuthorities, [None, None, None, None, None]),
];

/// What a decision needs of its TEXT: `Other` needs text for every action but `NoAction`, a referral needs text
/// whatever the reason, `NoAction` needs nothing.
fn text_needed(action: Action, reason: Reason) -> bool {
    match action {
        Action::NoAction => false,
        Action::ReferToAuthorities => true,
        _ => reason == Reason::Other,
    }
}

#[test]
fn r16_decisions_validate_against_their_report() {
    // Every (action, subject, reason) combination is tested, with and without the text, and pinned.
    let mut combinations = 0;
    for (action, row) in SUBJECT_TABLE {
        for (kind, subject_verdict) in KINDS.into_iter().zip(row) {
            for reason in Reason::ALL {
                let report = body_of(kind, reason);
                for with_text in [false, true] {
                    combinations += 1;
                    let got = decision(&report, action, reason, with_text).validate(&report);
                    let expected = match subject_verdict {
                        Some(e) => Err(e),
                        None if text_needed(action, reason) && !with_text => {
                            Err(DecisionError::NeedsText)
                        }
                        None => Ok(()),
                    };
                    assert_eq!(
                        got, expected,
                        "{action:?} on {kind:?}, reason {reason:?}, text {with_text}"
                    );
                }
            }
        }
    }
    assert_eq!(combinations, 6 * 5 * 7 * 2);
}

#[test]
fn r16_block_link_needs_a_link_subject() {
    for reason in Reason::ALL {
        for kind in KINDS {
            let report = body_of(kind, reason);
            let verdict =
                decision(&report, Action::BlockLink, Reason::Malware, true).validate(&report);
            match kind {
                Kind::Link | Kind::FileWithLink => assert_eq!(verdict, Ok(())),
                _ => assert_eq!(verdict, Err(DecisionError::NeedsLink), "{kind:?}"),
            }
        }
    }
}

#[test]
fn r16_other_needs_text_a_referral_needs_text_no_action_needs_nothing() {
    let report = link_body(Reason::Phishing);
    for action in Action::ALL {
        for reason in Reason::ALL {
            let none = decision(&report, action, reason, false).validate(&report);
            let some = decision(&report, action, reason, true).validate(&report);
            assert_eq!(some, Ok(()), "{action:?} {reason:?} with text");
            if text_needed(action, reason) {
                assert_eq!(none, Err(DecisionError::NeedsText), "{action:?} {reason:?}");
            } else {
                assert_eq!(none, Ok(()), "{action:?} {reason:?} needs no text");
            }
        }
    }
    // An empty text is no text.
    let mut d = decision(&report, Action::ReferToAuthorities, Reason::Csam, false);
    d.text = Some(text(""));
    assert_eq!(d.validate(&report), Err(DecisionError::NeedsText));
    // `NoAction` needs nothing, not even a matching subject or text, but it must be about this report.
    let mut d = decision(&report, Action::NoAction, Reason::Other, false);
    assert_eq!(d.validate(&report), Ok(()));
    d.report = rid(200);
    assert_eq!(d.validate(&report), Err(DecisionError::WrongReport));
}

#[test]
fn r16_a_decision_about_another_report_is_refused_before_anything_else() {
    let report = member_body(Reason::Csam);
    for action in Action::ALL {
        let mut d = decision(&report, action, Reason::Other, false);
        d.report = rid(99);
        assert_eq!(
            d.validate(&report),
            Err(DecisionError::WrongReport),
            "{action:?}"
        );
    }
}

#[test]
fn r16_a_text_longer_than_the_limit_is_refused() {
    let report = link_body(Reason::Phishing);
    let mut d = decision(&report, Action::BlockLink, Reason::Phishing, true);
    // A text made for the url field may be 2,048 bytes: longer than a decision's text may be.
    d.text = Some(Text::for_field(&"t".repeat(2_048), Field::Url).unwrap());
    assert_eq!(d.validate(&report), Err(DecisionError::TextTooLong));
    d.text = Some(Text::for_field(&"t".repeat(2_000), Field::Url).unwrap());
    assert_eq!(d.validate(&report), Ok(()));
    // The subject is checked before the text: a chat report cannot be blocked, whatever the text.
    let chat = chat_body(Reason::Phishing, vec![chat_record(1)]);
    let mut d = decision(&chat, Action::BlockLink, Reason::Other, false);
    d.text = None;
    assert_eq!(d.validate(&chat), Err(DecisionError::NeedsLink));
}
