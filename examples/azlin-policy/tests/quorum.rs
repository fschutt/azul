//! Check 6: the quorum counts distinct people, never one person's devices (planning/architecture.md
//! section 12 E1; section 5.10 row `admin_quorum`; section 5.9 `cosigs`). Every test names the rule
//! it holds. The guards `policy_quorum_counts_devices`, `policy_quorum_counts_apps`,
//! `policy_quorum_needs_one` and `policy_quorum_off_by_one` turn four of them off.

mod common;

use std::str::FromStr;

use azlin_policy::{
    apply, distinct_people, quorum_met, required_people, AccountKind, AdminQuorum, Policy,
    PolicyPatch, QuorumClass, QuorumShort, Signatory, MAX_ADMIN_QUORUM,
};
use azlin_seams::sim::Rng;
use azlin_seams::MemberId;
use common::{member, KINDS};

/// The five classes that the policy's `admin_quorum` guards.
const GUARDED: [QuorumClass; 5] = [
    QuorumClass::AdminChange,
    QuorumClass::PolicyChange,
    QuorumClass::EscrowChange,
    QuorumClass::EscrowOpen,
    QuorumClass::QuorumEnrollment,
];

fn business() -> Policy {
    Policy::defaults(AccountKind::Business)
}

#[test]
fn quorum_counts_distinct_people_never_devices() {
    // E1: five devices of one member are five signatures and ONE person.
    let alice = member(1);
    let five_devices = [Signatory::person(alice); 5];
    assert_eq!(distinct_people(&five_devices), 1);
    let refused = quorum_met(&business(), QuorumClass::AdminChange, &five_devices);
    assert_eq!(refused, Err(QuorumShort { have: 1, need: 2 }));

    // Two people with five devices each: ten signatures, two people.
    let bob = member(2);
    let mut ten = Vec::new();
    for _ in 0..5 {
        ten.push(Signatory::person(alice));
        ten.push(Signatory::person(bob));
    }
    assert_eq!(ten.len(), 10);
    assert_eq!(distinct_people(&ten), 2);
    assert_eq!(
        quorum_met(&business(), QuorumClass::AdminChange, &ten),
        Ok(())
    );

    // The count stops at 255 and never wraps: 300 different people are 255.
    let crowd: Vec<Signatory> = (0..300u16)
        .map(|n| {
            let mut bytes = [0u8; 16];
            bytes[..2].copy_from_slice(&n.to_be_bytes());
            Signatory::person(MemberId::from_bytes(bytes))
        })
        .collect();
    assert_eq!(distinct_people(&crowd), 255);
    // 255 and 256 entries of one person are one person.
    assert_eq!(distinct_people(&vec![Signatory::person(alice); 256]), 1);
    assert_eq!(distinct_people(&[]), 0);
}

#[test]
fn quorum_signer_counts_once_with_cosigners() {
    // The signer and the cosigners are one list: the signer counts, with the cosigners, once each.
    let signer = Signatory::person(member(1));
    let cosigner = Signatory::person(member(2));
    assert_eq!(distinct_people(&[signer, cosigner]), 2);
    assert_eq!(
        quorum_met(&business(), QuorumClass::PolicyChange, &[signer, cosigner]),
        Ok(())
    );
    // The signer alone is one person: short by one.
    assert_eq!(
        quorum_met(&business(), QuorumClass::PolicyChange, &[signer]),
        Err(QuorumShort { have: 1, need: 2 })
    );
    // The cosigner alone is one person too (who the signer is does not change the count).
    assert_eq!(
        quorum_met(&business(), QuorumClass::PolicyChange, &[cosigner]),
        Err(QuorumShort { have: 1, need: 2 })
    );
    // With a quorum of three: signer and two cosigners.
    let mut policy = business();
    policy.admin_quorum = AdminQuorum::new(3).expect("3");
    let third = Signatory::person(member(3));
    assert_eq!(
        quorum_met(&policy, QuorumClass::AdminChange, &[signer, cosigner]),
        Err(QuorumShort { have: 2, need: 3 })
    );
    assert_eq!(
        quorum_met(
            &policy,
            QuorumClass::AdminChange,
            &[signer, cosigner, third]
        ),
        Ok(())
    );
}

#[test]
fn quorum_duplicate_cosigner_counts_once() {
    let signer = Signatory::person(member(1));
    let cosigner = Signatory::person(member(2));
    // The same cosigner twice (two of its devices, or the same signature copied): one person.
    assert_eq!(distinct_people(&[signer, cosigner, cosigner]), 2);
    // The signer again among the cosigners (another device of the signer): still the signer.
    assert_eq!(distinct_people(&[signer, signer]), 1);
    assert_eq!(
        quorum_met(
            &business(),
            QuorumClass::EscrowOpen,
            &[signer, signer, signer]
        ),
        Err(QuorumShort { have: 1, need: 2 })
    );
    // Padding the list with copies never buys a person.
    let mut policy = business();
    policy.admin_quorum = AdminQuorum::new(3).expect("3");
    let padded = [signer, cosigner, cosigner, cosigner, signer, cosigner];
    assert_eq!(
        quorum_met(&policy, QuorumClass::EscrowChange, &padded),
        Err(QuorumShort { have: 2, need: 3 })
    );
}

#[test]
fn quorum_app_principal_never_counts() {
    let person = Signatory::person(member(1));
    let app = Signatory::app(member(9));
    let other_app = Signatory::app(member(8));
    assert_eq!(distinct_people(&[app]), 0);
    assert_eq!(distinct_people(&[person, app]), 1);
    assert_eq!(distinct_people(&[person, app, other_app, app]), 1);
    // A person and an app do not make a quorum of two.
    assert_eq!(
        quorum_met(&business(), QuorumClass::AdminChange, &[person, app]),
        Err(QuorumShort { have: 1, need: 2 })
    );
    // An app alone does not even make a quorum of one (a personal account's signer is a person).
    let personal = Policy::defaults(AccountKind::Personal);
    assert_eq!(
        quorum_met(&personal, QuorumClass::AdminChange, &[app]),
        Err(QuorumShort { have: 0, need: 1 })
    );
    // A member id that is listed as a person and as an app is no consistent fact: it counts for
    // nobody (a quorum is never over-counted).
    let both_person = Signatory::person(member(5));
    let both_app = Signatory::app(member(5));
    assert_eq!(distinct_people(&[both_person, both_app]), 0);
    assert_eq!(distinct_people(&[both_app, both_person, person]), 1);
}

#[test]
fn quorum_two_for_business_admin_change() {
    let policy = business();
    let one = [Signatory::person(member(1))];
    let two = [Signatory::person(member(1)), Signatory::person(member(2))];
    let three = [
        Signatory::person(member(1)),
        Signatory::person(member(2)),
        Signatory::person(member(3)),
    ];
    // The five classes of the table: admin changes, policy, escrow, escrow_open, quorum enrollments.
    for class in GUARDED {
        assert_eq!(required_people(&policy, class), 2, "{class}");
        assert_eq!(
            quorum_met(&policy, class, &one),
            Err(QuorumShort { have: 1, need: 2 }),
            "{class}"
        );
        // Exactly two is enough (the boundary), and more is too.
        assert_eq!(quorum_met(&policy, class, &two), Ok(()), "{class}");
        assert_eq!(quorum_met(&policy, class, &three), Ok(()), "{class}");
        assert_eq!(
            quorum_met(&policy, class, &[]),
            Err(QuorumShort { have: 0, need: 2 }),
            "{class}"
        );
    }
    // Any other change needs one person.
    assert_eq!(required_people(&policy, QuorumClass::Other), 1);
    assert_eq!(quorum_met(&policy, QuorumClass::Other, &one), Ok(()));
}

#[test]
fn quorum_one_for_personal_and_family() {
    for kind in [
        AccountKind::Personal,
        AccountKind::Family,
        AccountKind::Team,
    ] {
        let policy = Policy::defaults(kind);
        for class in QuorumClass::ALL {
            assert_eq!(required_people(&policy, *class), 1, "{kind} {class}");
            assert_eq!(
                quorum_met(&policy, *class, &[Signatory::person(member(1))]),
                Ok(()),
                "{kind} {class}: the signer alone"
            );
            assert_eq!(
                quorum_met(&policy, *class, &[]),
                Err(QuorumShort { have: 0, need: 1 }),
                "{kind} {class}: nobody"
            );
        }
    }
}

#[test]
fn quorum_short_names_have_and_need() {
    let short = quorum_met(
        &business(),
        QuorumClass::AdminChange,
        &[Signatory::person(member(1))],
    )
    .expect_err("one person is short of two");
    assert_eq!(short.have, 1);
    assert_eq!(short.need, 2);
    let text = short.to_string();
    assert!(text.contains('1') && text.contains('2'), "{text}");
    assert!(text.contains("quorum"), "{text}");
    // And for a quorum of five with three people.
    let mut policy = business();
    policy.admin_quorum = AdminQuorum::new(5).expect("5");
    let signers: Vec<Signatory> = (1..=3).map(|n| Signatory::person(member(n))).collect();
    let short = quorum_met(&policy, QuorumClass::EscrowOpen, &signers).expect_err("short");
    assert_eq!((short.have, short.need), (3, 5));
}

#[test]
fn the_quorum_follows_a_changed_admin_quorum() {
    // `admin_quorum_set` is a patch: the quorum of the next change is the new number.
    let mut patch = PolicyPatch::none();
    patch.admin_quorum = Some(AdminQuorum::new(4).expect("4"));
    let policy = apply(&Policy::defaults(AccountKind::Personal), &patch);
    for class in GUARDED {
        assert_eq!(required_people(&policy, class), 4);
    }
    assert_eq!(required_people(&policy, QuorumClass::Other), 1);
}

#[test]
fn an_admin_quorum_is_never_zero_and_never_above_the_limit() {
    assert!(AdminQuorum::new(0).is_err());
    assert_eq!(AdminQuorum::new(0).expect_err("0").people, 0);
    assert!(AdminQuorum::new(MAX_ADMIN_QUORUM + 1).is_err());
    assert!(AdminQuorum::new(255).is_err());
    assert_eq!(MAX_ADMIN_QUORUM, 8);
    for people in 1..=MAX_ADMIN_QUORUM {
        let quorum = AdminQuorum::new(people).expect("in range");
        assert_eq!(quorum.get(), people);
    }
    assert!(AdminQuorum::new(0)
        .expect_err("0")
        .to_string()
        .contains('0'));
    // So no policy has a quorum of nothing: every kind, every class, needs at least one person.
    for kind in KINDS {
        for class in QuorumClass::ALL {
            assert!(required_people(&Policy::defaults(kind), *class) >= 1);
        }
    }
}

#[test]
fn quorum_classes_have_strict_words() {
    let words = [
        "admin_change",
        "policy_change",
        "escrow_change",
        "escrow_open",
        "quorum_enrollment",
        "other",
    ];
    assert_eq!(QuorumClass::ALL.len(), words.len());
    for (class, word) in QuorumClass::ALL.iter().zip(words) {
        assert_eq!(class.as_str(), word);
        assert_eq!(class.to_string(), word);
        assert_eq!(QuorumClass::from_str(word), Ok(*class));
        assert!(QuorumClass::from_str(&word.to_uppercase()).is_err());
        assert!(QuorumClass::from_str(&format!(" {word}")).is_err());
    }
    assert!(QuorumClass::from_str("").is_err());
    assert!(QuorumClass::from_str("admin").is_err());
}

/// The count by another route: a member counts when some entry names it as a person and no entry
/// names it as an app. (`distinct_people` collects two sets; this asks each id of the list.)
fn model(signers: &[Signatory]) -> usize {
    let mut ids: Vec<MemberId> = signers.iter().map(|s| s.member).collect();
    ids.sort();
    ids.dedup();
    ids.iter()
        .filter(|id| {
            signers
                .iter()
                .any(|s| s.member == **id && s.kind == azlin_policy::MemberKind::Person)
                && !signers
                    .iter()
                    .any(|s| s.member == **id && s.kind == azlin_policy::MemberKind::App)
        })
        .count()
}

#[test]
fn distinct_people_equals_the_model_for_3000_seeded_lists() {
    let mut rng = Rng::new(0x0001_0000_0E10);
    for round in 0..3000 {
        let len = rng.below(40).expect("a positive range");
        let signers: Vec<Signatory> = (0..len)
            .map(|_| {
                let id = member(rng.below(12).expect("a positive range") as u8);
                if rng.chance(1, 4) {
                    Signatory::app(id)
                } else {
                    Signatory::person(id)
                }
            })
            .collect();
        assert_eq!(
            distinct_people(&signers) as usize,
            model(&signers),
            "round {round}: {signers:?}"
        );
        // The order of the signatures and a copy of the list change nothing.
        let mut shuffled = signers.clone();
        rng.shuffle(&mut shuffled);
        shuffled.extend(signers.iter().copied());
        assert_eq!(
            distinct_people(&shuffled),
            distinct_people(&signers),
            "round {round}"
        );
        // A quorum is met exactly when the count reaches the need.
        let policy = business();
        let people = distinct_people(&signers);
        assert_eq!(
            quorum_met(&policy, QuorumClass::AdminChange, &signers).is_ok(),
            people >= 2
        );
    }
}
