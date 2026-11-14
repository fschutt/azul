//! Checks R17 and R18 of lane W1-S46a: the statement of reasons (DSA Art. 17) and the notifier's notice
//! (Art. 16(5)), planning/architecture.md section 3.9 and 7.10. Every test enters through the public API.

mod common;

use std::collections::HashSet;

use azlin_cbor::{DecodeError, ShapeError, Value};
use azlin_reports::{
    statement_of, statement_of_step, Action, AutoStep, BodyError, Decision, Facts, Field, Ground,
    NotifierNotice, Outcome, Reason, Redress, Restriction, StatementOfReasons, Text,
};

use common::*;

fn decision(action: Action, reason: Reason, with_text: bool) -> Decision {
    Decision {
        report: rid(42),
        action,
        reason,
        text: if with_text {
            Some(text("the operator's words about the account"))
        } else {
            None
        },
        day: day(321),
    }
}

/// What restriction each action has, written out.
const RESTRICTION_OF: [(Action, Restriction); 6] = [
    (Action::NoAction, Restriction::NoRestriction),
    (Action::BlockLink, Restriction::LinkBlocked),
    (Action::LockAccount, Restriction::AccountReadOnly),
    (Action::Ban, Restriction::AccountBanned),
    (Action::LegalHold, Restriction::DataKept),
    (Action::ReferToAuthorities, Restriction::Referred),
];

/// What ground each reason rests on, written out.
const GROUND_OF: [(Reason, Ground); 7] = [
    (Reason::IllegalContent, Ground::IllegalContent),
    (Reason::Csam, Ground::IllegalContent),
    (Reason::Terrorism, Ground::IllegalContent),
    (Reason::Malware, Ground::Terms),
    (Reason::Phishing, Ground::Terms),
    (Reason::Copyright, Ground::IllegalContent),
    (Reason::Other, Ground::Terms),
];

fn restriction_of(action: Action) -> Restriction {
    RESTRICTION_OF
        .iter()
        .find(|(a, _)| *a == action)
        .map(|(_, r)| *r)
        .unwrap()
}

fn ground_of(reason: Reason) -> Ground {
    GROUND_OF
        .iter()
        .find(|(r, _)| *r == reason)
        .map(|(_, g)| *g)
        .unwrap()
}

// ---------------------------------------------------------------- R17

#[test]
fn r17_the_statement_of_reasons_has_every_art_17_element() {
    for action in Action::ALL {
        for reason in Reason::ALL {
            for with_text in [false, true] {
                let d = decision(action, reason, with_text);
                let s = statement_of(&d, false);
                let what = format!("{action:?} / {reason:?} / text {with_text}");
                // (a) what was restricted
                assert_eq!(s.restriction, restriction_of(action), "{what}");
                assert!(Restriction::ALL.contains(&s.restriction));
                // (b) the facts and circumstances: the reason and the text
                assert_eq!(
                    s.facts,
                    Facts::Notice {
                        reason,
                        text: d.text.clone()
                    },
                    "{what}"
                );
                // (c) whether automated means decided: not an operator's decision
                assert!(
                    !s.automated,
                    "{what}: an operator's decision is not automated"
                );
                // (d) the ground
                let ground = if action == Action::ReferToAuthorities {
                    Ground::IllegalContent
                } else {
                    ground_of(reason)
                };
                assert_eq!(s.ground, ground, "{what}");
                // (f) how to object: a reply through the app and the courts, never empty
                assert_eq!(s.redress, Redress::STANDARD, "{what}");
                assert!(s.redress.reply_in_app && s.redress.courts && s.redress.bits() != 0);
                assert_eq!(s.day, d.day);
                // The caller says automated: the statement carries it.
                assert!(
                    statement_of(&d, true).automated,
                    "{what}: `automated` was dropped"
                );
                // The encoded notice round-trips.
                let bytes = s.encode().unwrap();
                assert_eq!(StatementOfReasons::decode(&bytes).unwrap(), s, "{what}");
                let told =
                    StatementOfReasons::decode(&statement_of(&d, true).encode().unwrap()).unwrap();
                assert!(
                    told.automated,
                    "{what}: `automated` did not survive the notice"
                );
            }
        }
    }
}

#[test]
fn r17_automated_is_true_exactly_for_the_counting_engines_step() {
    let link = link(3);
    let pause = AutoStep::PauseLink { link, devices: 3 };
    let off = AutoStep::DisableLinks {
        account: acct(2),
        devices: 5,
    };
    let s = statement_of_step(&pause, day(321));
    assert!(s.automated);
    assert_eq!(s.restriction, Restriction::LinkPaused);
    assert_eq!(s.facts, Facts::Counted { devices: 3 });
    assert_eq!(s.ground, Ground::AutomaticSafeguard);
    assert_eq!(s.redress, Redress::STANDARD);
    assert_eq!(s.day, day(321));
    let s2 = statement_of_step(&off, day(5));
    assert!(s2.automated);
    assert_eq!(s2.restriction, Restriction::PublicLinksDisabled);
    assert_eq!(s2.facts, Facts::Counted { devices: 5 });
    for s in [s, s2] {
        let bytes = s.encode().unwrap();
        assert_eq!(StatementOfReasons::decode(&bytes).unwrap(), s);
    }
    // No operator decision is automated.
    for action in Action::ALL {
        assert!(!statement_of(&decision(action, Reason::Other, true), false).automated);
    }
}

#[test]
fn r17_only_the_restrictions_the_account_sees_are_told_to_it() {
    // Provisional (drift X-DELTA-W1S46A-12): a legal hold and a referral restrict nothing the account notices,
    // and telling it could warn the person an authority looks at.
    let told: Vec<Restriction> = Restriction::ALL
        .into_iter()
        .filter(|r| r.is_visible_to_account())
        .collect();
    assert_eq!(
        told,
        vec![
            Restriction::LinkPaused,
            Restriction::PublicLinksDisabled,
            Restriction::LinkBlocked,
            Restriction::AccountReadOnly,
            Restriction::AccountBanned,
        ]
    );
}

const GOLDEN_STATEMENT: &str = "a801010201030404a20101020405f406020705081864";
const GOLDEN_STEP: &str = "a801010201030204a20102020305f50603070508190141";
const GOLDEN_NOTIFIER: &str = "a501010202035042424242424242424242424242424242040205190141";

#[test]
fn r17_the_golden_notices_never_change() {
    // Made by an independent encoder (scratch golden.py). The statement reads by eye: a8 (map of 8) 01 01
    // (version 1) 02 01 (kind: statement) 03 04 (LinkBlocked) 04 a2 01 01 02 04 (facts: a notice, reason Malware)
    // 05 f4 (not automated) 06 02 (Terms) 07 05 (reply + courts) 08 18 64 (day 100).
    let s = StatementOfReasons {
        restriction: Restriction::LinkBlocked,
        facts: Facts::Notice {
            reason: Reason::Malware,
            text: None,
        },
        automated: false,
        ground: Ground::Terms,
        redress: Redress::STANDARD,
        day: day(100),
    };
    assert_eq!(hex(&s.encode().unwrap()), GOLDEN_STATEMENT);
    assert_eq!(
        StatementOfReasons::decode(&unhex(GOLDEN_STATEMENT)).unwrap(),
        s
    );
    let step = statement_of_step(
        &AutoStep::PauseLink {
            link: link(1),
            devices: 3,
        },
        day(321),
    );
    assert_eq!(hex(&step.encode().unwrap()), GOLDEN_STEP);
    assert_eq!(
        StatementOfReasons::decode(&unhex(GOLDEN_STEP)).unwrap(),
        step
    );
    let n = NotifierNotice {
        receipt: rid(0x42),
        outcome: Outcome::ActionTaken,
        day: day(321),
    };
    assert_eq!(hex(&n.encode().unwrap()), GOLDEN_NOTIFIER);
    assert_eq!(NotifierNotice::decode(&unhex(GOLDEN_NOTIFIER)).unwrap(), n);
}

fn statement_pairs() -> Vec<(u64, Value)> {
    vec![
        (1, Value::Uint(1)),
        (2, Value::Uint(1)),
        (3, Value::Uint(4)),
        (4, raw_map(vec![(1, Value::Uint(1)), (2, Value::Uint(4))])),
        (5, Value::Bool(false)),
        (6, Value::Uint(2)),
        (7, Value::Uint(5)),
        (8, Value::Uint(100)),
    ]
}

fn set(pairs: &mut Vec<(u64, Value)>, key: u64, value: Value) {
    pairs.retain(|(k, _)| *k != key);
    pairs.push((key, value));
}

#[test]
fn r17_the_encoded_notice_is_strict() {
    let bytes_of = |pairs: &[(u64, Value)]| cbor(&raw_map(pairs.to_vec()));
    let ok = bytes_of(&statement_pairs());
    assert_eq!(
        hex(&ok),
        GOLDEN_STATEMENT,
        "the control is the golden statement"
    );
    assert!(StatementOfReasons::decode(&ok).is_ok());

    let mut cases: Vec<(&str, Vec<u8>, BodyError)> = Vec::new();
    let mut tamper = |name: &'static str, expected: BodyError, f: &dyn Fn(&mut Pairs)| {
        let mut pairs = statement_pairs();
        f(&mut pairs);
        cases.push((name, bytes_of(&pairs), expected));
    };
    tamper("a wrong version", BodyError::Version(2), &|p| {
        set(p, 1, Value::Uint(2))
    });
    tamper(
        "an unknown restriction",
        BodyError::UnknownRestriction(99),
        &|p| set(p, 3, Value::Uint(99)),
    );
    tamper("restriction 0", BodyError::UnknownRestriction(0), &|p| {
        set(p, 3, Value::Uint(0))
    });
    tamper("an unknown facts kind", BodyError::UnknownFacts(9), &|p| {
        set(p, 4, raw_map(vec![(1, Value::Uint(9))]))
    });
    tamper(
        "an unknown reason in the facts",
        BodyError::UnknownReason(9),
        &|p| {
            set(
                p,
                4,
                raw_map(vec![(1, Value::Uint(1)), (2, Value::Uint(9))]),
            )
        },
    );
    tamper("an unknown ground", BodyError::UnknownGround(9), &|p| {
        set(p, 6, Value::Uint(9))
    });
    tamper("redress without a path", BodyError::BadRedress(0), &|p| {
        set(p, 7, Value::Uint(0))
    });
    tamper(
        "redress with an undefined bit",
        BodyError::BadRedress(8),
        &|p| set(p, 7, Value::Uint(8)),
    );
    tamper(
        "automated of the wrong type",
        BodyError::Shape(ShapeError::WrongType(5)),
        &|p| set(p, 5, Value::Uint(1)),
    );
    tamper(
        "a missing day",
        BodyError::Shape(ShapeError::Missing(8)),
        &|p| p.retain(|(k, _)| *k != 8),
    );
    tamper(
        "a day beyond u32",
        BodyError::Shape(ShapeError::WrongType(8)),
        &|p| set(p, 8, Value::Uint(1 << 40)),
    );
    tamper(
        "an unknown key in crit",
        BodyError::Shape(ShapeError::CritUnknown(99)),
        &|p| {
            set(p, 99, Value::Null);
            set(p, 0, Value::Array(vec![Value::Uint(99)]));
        },
    );
    let mut trailing = ok.clone();
    trailing.push(0);
    cases.push((
        "trailing bytes",
        trailing,
        BodyError::Cbor(DecodeError::TrailingBytes),
    ));
    cases.push((
        "a float",
        vec![0xa1, 0x01, 0xf9, 0x3c, 0x00],
        BodyError::Cbor(DecodeError::DisallowedType),
    ));
    cases.push((
        "truncated",
        ok[..ok.len() - 1].to_vec(),
        BodyError::Cbor(DecodeError::Truncated),
    ));
    cases.push((
        "a body over 4 KiB",
        vec![0; 4 * 1024 + 1],
        BodyError::Cbor(DecodeError::InputTooLarge),
    ));
    cases.push((
        "a notifier notice",
        unhex(GOLDEN_NOTIFIER),
        BodyError::WrongNotice(2),
    ));

    let mut seen: HashSet<BodyError> = HashSet::new();
    for (name, bytes, expected) in &cases {
        assert_eq!(
            StatementOfReasons::decode(bytes).unwrap_err(),
            *expected,
            "the malformed statement '{name}' failed with another error"
        );
        assert!(
            seen.insert(*expected),
            "'{name}' shares its error with another case"
        );
    }

    // Unknown keys that are not in crit are ignored.
    let mut newer = statement_pairs();
    set(
        &mut newer,
        99,
        Value::Text("a field of a newer writer".into()),
    );
    assert_eq!(
        StatementOfReasons::decode(&bytes_of(&newer)),
        StatementOfReasons::decode(&ok),
        "an unknown key changed what was read"
    );
    // A statement is refused where a notifier notice is asked for.
    assert_eq!(NotifierNotice::decode(&ok), Err(BodyError::WrongNotice(1)));
}

#[test]
fn r17_a_statement_whose_text_is_longer_than_the_limit_is_not_written() {
    let long = Text::for_field(&"t".repeat(2_048), Field::Url).unwrap();
    let s = StatementOfReasons {
        restriction: Restriction::Referred,
        facts: Facts::Notice {
            reason: Reason::Csam,
            text: Some(long),
        },
        automated: false,
        ground: Ground::IllegalContent,
        redress: Redress::STANDARD,
        day: day(1),
    };
    assert_eq!(s.encode(), Err(BodyError::TooLong(Field::Text)));
}

// ---------------------------------------------------------------- R18

#[test]
fn r18_the_notifier_gets_the_receipt_and_the_decision() {
    for action in Action::ALL {
        for reason in Reason::ALL {
            let d = decision(action, reason, true);
            let n = NotifierNotice::for_decision(&d);
            assert_eq!(
                n.receipt, d.report,
                "the notice carries the receipt of the report"
            );
            let word = if action == Action::NoAction {
                Outcome::NoAction
            } else {
                Outcome::ActionTaken
            };
            assert_eq!(n.outcome, word, "{action:?}");
            assert_eq!(n.day, d.day);
            let bytes = n.encode().unwrap();
            assert_eq!(NotifierNotice::decode(&bytes).unwrap(), n);
            // Nothing the operator wrote, and nothing of the account, is in the bytes.
            let printed = String::from_utf8_lossy(&bytes);
            assert!(
                !printed.contains("operator"),
                "the operator's text reached the notifier"
            );
        }
    }
    // The notice carries nothing about the reported account beyond the outcome word: two decisions with the
    // same word and different measures, reasons and texts give the same bytes.
    let a = NotifierNotice::for_decision(&decision(Action::Ban, Reason::Csam, true));
    let b = NotifierNotice::for_decision(&decision(Action::BlockLink, Reason::Malware, false));
    let c =
        NotifierNotice::for_decision(&decision(Action::ReferToAuthorities, Reason::Other, true));
    assert_eq!(a.encode().unwrap(), b.encode().unwrap());
    assert_eq!(a.encode().unwrap(), c.encode().unwrap());
    let none = NotifierNotice::for_decision(&decision(Action::NoAction, Reason::Csam, true));
    assert_ne!(a.encode().unwrap(), none.encode().unwrap());
    // Exactly three fields: the pattern breaks if a field is added.
    let NotifierNotice {
        receipt,
        outcome,
        day: d,
    } = a;
    assert_eq!(
        (receipt, outcome, d),
        (rid(42), Outcome::ActionTaken, day(321))
    );
}

#[test]
fn r18_the_notifier_notice_is_strict() {
    let ok = unhex(GOLDEN_NOTIFIER);
    let pairs = || -> Vec<(u64, Value)> {
        vec![
            (1, Value::Uint(1)),
            (2, Value::Uint(2)),
            (3, Value::Bytes(vec![0x42; 16])),
            (4, Value::Uint(2)),
            (5, Value::Uint(321)),
        ]
    };
    let bytes_of = |p: &[(u64, Value)]| cbor(&raw_map(p.to_vec()));
    assert_eq!(bytes_of(&pairs()), ok);
    let mut cases: Vec<(&str, Vec<u8>, BodyError)> = Vec::new();
    let mut tamper = |name: &'static str, expected: BodyError, f: &dyn Fn(&mut Pairs)| {
        let mut p = pairs();
        f(&mut p);
        cases.push((name, bytes_of(&p), expected));
    };
    tamper("a wrong version", BodyError::Version(3), &|p| {
        set(p, 1, Value::Uint(3))
    });
    tamper("an unknown outcome", BodyError::UnknownOutcome(7), &|p| {
        set(p, 4, Value::Uint(7))
    });
    tamper("outcome 0", BodyError::UnknownOutcome(0), &|p| {
        set(p, 4, Value::Uint(0))
    });
    tamper(
        "a short receipt",
        BodyError::Shape(ShapeError::WrongLength(3)),
        &|p| set(p, 3, Value::Bytes(vec![1; 15])),
    );
    tamper(
        "a missing receipt",
        BodyError::Shape(ShapeError::Missing(3)),
        &|p| p.retain(|(k, _)| *k != 3),
    );
    tamper(
        "a day beyond u32",
        BodyError::Shape(ShapeError::WrongType(5)),
        &|p| set(p, 5, Value::Uint(1 << 33)),
    );
    cases.push((
        "a statement",
        unhex(GOLDEN_STATEMENT),
        BodyError::WrongNotice(1),
    ));
    let mut trailing = ok.clone();
    trailing.push(1);
    cases.push((
        "trailing bytes",
        trailing,
        BodyError::Cbor(DecodeError::TrailingBytes),
    ));
    let mut seen = HashSet::new();
    for (name, bytes, expected) in &cases {
        assert_eq!(
            NotifierNotice::decode(bytes).unwrap_err(),
            *expected,
            "'{name}'"
        );
        assert!(seen.insert(*expected), "'{name}' shares its error");
    }
    // Every outcome word round-trips.
    for outcome in Outcome::ALL {
        let n = NotifierNotice {
            receipt: rid(1),
            outcome,
            day: day(9),
        };
        assert_eq!(NotifierNotice::decode(&n.encode().unwrap()).unwrap(), n);
    }
}
