//! Check 7 of the brief: the limits. "Rate limits per person" (planning/architecture.md section
//! 7.5), one pending request at a time and three openings a day (the estimates of the lane's brief
//! and of COMMON-W1R R13), counted per PERSON, never per device.

mod common;

use azlin_enroll::{
    cancel, expire, release, Canceller, LimitRefusal, LimitsInvalid, OpenLimits, OpenRefusal,
    Opener, OpenerKind, Path, Release, StampCheck, DAILY_WINDOW, MAX_LEDGERS, STANDARD_PENDING_MAX,
    STANDARD_PER_DAY,
};
use common::{
    at, device, hold_secs, mark_released, member, open, Scene, DAY, HOUR, NEW_DEVICE, PERSON,
};

/// Opens a recovery-code request for `person` from `new_device`, with the scene's policy and the
/// facts of `person`.
fn open_for(
    scene: &mut Scene,
    person: u8,
    new_device: u8,
    now: u64,
) -> Result<azlin_enroll::Request, OpenRefusal> {
    let mut facts = scene.facts.clone();
    facts.person = member(person);
    open(
        &facts,
        &scene.policy,
        Path::RecoveryCode,
        Opener::person(device(new_device)),
        at(now),
        &mut scene.limits,
    )
    .map(|(request, _)| request)
}

#[test]
fn standard_limits_are_one_pending_and_three_a_day() {
    assert_eq!(STANDARD_PENDING_MAX, 1);
    assert_eq!(STANDARD_PER_DAY, 3);
    let limits = OpenLimits::standard();
    assert_eq!(limits.pending_max(), 1);
    assert_eq!(limits.per_day(), 3);
    assert_eq!(DAILY_WINDOW.as_secs(), DAY);
    assert_eq!(OpenLimits::new(0, 3).unwrap_err(), LimitsInvalid);
    assert_eq!(OpenLimits::new(1, 0).unwrap_err(), LimitsInvalid);
    assert_eq!(OpenLimits::new(2, 5).unwrap().pending_max(), 2);
}

#[test]
fn second_open_while_one_is_pending_refused() {
    let mut scene = Scene::full();
    let first = open_for(&mut scene, PERSON, NEW_DEVICE, 0).expect("the first opens");
    assert_eq!(scene.limits.pending_of(member(PERSON), at(1)), 1);
    // A second request for the same person, on any path, while the first is open.
    assert_eq!(
        open_for(&mut scene, PERSON, 101, 10).unwrap_err(),
        OpenRefusal::Limit(LimitRefusal::PendingLimit { pending: 1, max: 1 })
    );
    assert!(matches!(
        scene.try_open(Path::EscrowRecovery, at(10)),
        Err(OpenRefusal::Limit(LimitRefusal::PendingLimit { .. }))
    ));
    // The refusals counted nothing: one opening so far.
    assert_eq!(scene.limits.opened_today(member(PERSON), at(11)), 1);
    // Cancelled: the slot is free once the group tells the ledger.
    let mut first = first;
    cancel(
        &mut first,
        &scene.facts,
        Canceller::Admin(member(10)),
        at(20),
    )
    .unwrap();
    // The ledger is told when the request is no longer open; until then the slot is taken.
    assert!(open_for(&mut scene, PERSON, 101, 30).is_err());
    scene.limits.closed(&first);
    assert_eq!(scene.limits.pending_of(member(PERSON), at(31)), 0);
    let second = open_for(&mut scene, PERSON, 101, 40).expect("the slot is free");
    assert_ne!(first.id(), second.id());
    // A request that is still open frees nothing: `closed` cannot be used to get round the limit.
    scene.limits.closed(&second);
    assert_eq!(scene.limits.pending_of(member(PERSON), at(41)), 1);
    assert!(open_for(&mut scene, PERSON, 102, 50).is_err());
}

#[test]
fn a_released_and_an_expired_request_free_their_slot_too() {
    let mut scene = Scene::full();
    let mut request = open_for(&mut scene, PERSON, NEW_DEVICE, 0).unwrap();
    scene.complete(&mut request, at(0));
    let grant = match release(
        &request,
        at(hold_secs(Path::RecoveryCode)),
        StampCheck::Agreed,
    ) {
        Release::Granted(grant) => grant,
        other => panic!("{other:?}"),
    };
    mark_released(&mut request, &scene.facts, grant).unwrap();
    scene.limits.closed(&request);
    assert!(open_for(&mut scene, PERSON, 101, 50 * HOUR).is_ok());
    // An expired request: swept, then closed.
    let mut scene = Scene::full();
    let mut request = open_for(&mut scene, PERSON, NEW_DEVICE, 0).unwrap();
    let late = request.expires_at().as_secs() - common::BASE + 1;
    assert!(expire(&mut request, at(late), StampCheck::Agreed));
    scene.limits.closed(&request);
    assert_eq!(scene.limits.pending_of(member(PERSON), at(late)), 0);
}

#[test]
fn a_slot_nobody_freed_frees_itself_when_the_request_would_have_expired() {
    // The group forgot to tell the ledger: the person is not blocked for ever.
    let mut scene = Scene::full();
    let request = open_for(&mut scene, PERSON, NEW_DEVICE, 0).unwrap();
    let still = request.expires_at().as_secs() - common::BASE;
    assert!(
        open_for(&mut scene, PERSON, 101, still).is_err(),
        "still pending at its expiry"
    );
    // Two days (the hold) and seven more days later the request is gone and so is its slot ...
    assert_eq!(scene.limits.pending_of(member(PERSON), at(still + 1)), 0);
    // ... but the three openings of a day are by then long gone too.
    assert!(open_for(&mut scene, PERSON, 101, still + 1).is_ok());
}

#[test]
fn daily_limit_per_person() {
    let mut scene = Scene::full();
    // Three openings in a day, each one cancelled and closed before the next.
    for (n, minute) in [0u64, 10, 20].into_iter().enumerate() {
        let mut request = open_for(&mut scene, PERSON, 100 + n as u8, minute * 60).unwrap();
        cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(10)),
            at(minute * 60 + 1),
        )
        .unwrap();
        scene.limits.closed(&request);
    }
    assert_eq!(scene.limits.opened_today(member(PERSON), at(30 * 60)), 3);
    // The fourth in the same day: the daily limit, though nothing is pending.
    assert_eq!(scene.limits.pending_of(member(PERSON), at(30 * 60)), 0);
    assert_eq!(
        open_for(&mut scene, PERSON, 110, 30 * 60).unwrap_err(),
        OpenRefusal::Limit(LimitRefusal::DailyLimit { opened: 3, max: 3 })
    );
    // A rolling 24 hours: still refused one second before the first opening is a day old ...
    assert!(open_for(&mut scene, PERSON, 110, DAY - 1).is_err());
    // ... and open at the second the first one is a day old (the others are not yet).
    let request =
        open_for(&mut scene, PERSON, 110, DAY).expect("the first opening left the window");
    assert_eq!(scene.limits.opened_today(member(PERSON), at(DAY)), 3);
    let mut request = request;
    cancel(
        &mut request,
        &scene.facts,
        Canceller::Admin(member(10)),
        at(DAY + 1),
    )
    .unwrap();
    scene.limits.closed(&request);
    // The second opening (minute 10) leaves the window at DAY + 600.
    assert!(open_for(&mut scene, PERSON, 111, DAY + 599).is_err());
    assert!(open_for(&mut scene, PERSON, 111, DAY + 600).is_ok());
    // The number is the ledger's: a ledger with another number counts another number.
    let mut loose = Scene::full();
    loose.limits = OpenLimits::new(1, 5).unwrap();
    for n in 0..5u8 {
        let mut request = open_for(&mut loose, PERSON, 100 + n, u64::from(n)).unwrap();
        cancel(
            &mut request,
            &loose.facts,
            Canceller::Admin(member(10)),
            at(10),
        )
        .unwrap();
        loose.limits.closed(&request);
    }
    assert_eq!(
        open_for(&mut loose, PERSON, 120, 20).unwrap_err(),
        OpenRefusal::Limit(LimitRefusal::DailyLimit { opened: 5, max: 5 })
    );
}

#[test]
fn limits_are_per_person_not_per_device() {
    let mut scene = Scene::full();
    // The same person from three different new devices: one request pending, the rest refused.
    open_for(&mut scene, PERSON, 100, 0).unwrap();
    for new_device in [101u8, 102, 103] {
        assert!(
            matches!(
                open_for(&mut scene, PERSON, new_device, 5),
                Err(OpenRefusal::Limit(LimitRefusal::PendingLimit { .. }))
            ),
            "device {new_device} is another device of the same person"
        );
    }
    // The same NEW device for two different people: the people do not share a budget.
    for person in [2u8, 3, 4] {
        assert!(
            open_for(&mut scene, person, 100, 5).is_ok(),
            "person {person} has a budget of their own"
        );
    }
    assert_eq!(scene.limits.tracked_ledgers(), 4);
    for person in [PERSON, 2, 3, 4] {
        assert_eq!(scene.limits.pending_of(member(person), at(6)), 1);
        assert_eq!(scene.limits.opened_today(member(person), at(6)), 1);
    }
    // The helpdesk or an admin who opens for somebody uses a budget of that somebody (the one of
    // their kind of opener), never a budget of their own.
    let mut scene = Scene::full();
    scene.try_open(Path::EscrowRecovery, at(0)).unwrap();
    assert!(matches!(
        scene.try_open(Path::EscrowRecovery, at(1)),
        Err(OpenRefusal::Limit(LimitRefusal::PendingLimit { .. }))
    ));
    let helpdesk = Opener::helpdesk(member(common::HELPDESK), device(NEW_DEVICE));
    let refused = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowRecovery,
        helpdesk,
        at(2),
        &mut scene.limits,
    );
    assert!(
        refused.is_ok(),
        "the helpdesk's budget is the helpdesk's own: {refused:?}"
    );
    let second_helpdesk = Opener::helpdesk(member(common::HELPDESK + 1), device(101));
    let refused = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowRecovery,
        second_helpdesk,
        at(3),
        &mut scene.limits,
    );
    assert!(
        matches!(
            refused,
            Err(OpenRefusal::Limit(LimitRefusal::PendingLimit { .. }))
        ),
        "another helpdesk member opens for the same person and meets the same budget: {refused:?}"
    );
}

/// The person with this number, for as many people as the ledger can hold.
fn person_number(n: usize) -> azlin_seams::MemberId {
    let mut bytes = [0u8; 16];
    bytes[..8].copy_from_slice(&(n as u64).to_le_bytes());
    bytes[15] = 0xEE;
    azlin_seams::MemberId::from_bytes(bytes)
}

#[test]
fn a_lower_kind_of_opener_never_uses_up_the_budget_of_a_higher_one() {
    // A helpdesk member can open a recovery request for any member with no evidence at all, and
    // nothing of a person who opens from a new device is verified. If their openings shared a
    // budget with the openings that admins sign and with the approving device of the person, they
    // would starve them; so each kind of opener has a budget of its own.
    let mut scene = Scene::full();
    let helpdesk = |n: u8| Opener::helpdesk(member(common::HELPDESK), device(n));
    let by_helpdesk = |scene: &mut Scene, n: u8, now: u64| {
        open(
            &scene.facts,
            &scene.policy,
            Path::EscrowRecovery,
            helpdesk(n),
            at(now),
            &mut scene.limits,
        )
    };
    by_helpdesk(&mut scene, 101, 0).expect("the first helpdesk request opens");
    assert_eq!(
        by_helpdesk(&mut scene, 102, 1).unwrap_err(),
        OpenRefusal::Limit(LimitRefusal::PendingLimit { pending: 1, max: 1 })
    );
    // The person's own recovery, the person's own device, the quorum enrollment and the opening
    // that two admins signed all still open, each for its own budget.
    scene
        .try_open(Path::RecoveryCode, at(2))
        .expect("the person's own recovery");
    scene
        .try_open(Path::OwnDevice, at(3))
        .expect("the approving device of the person");
    scene
        .try_open(Path::EscrowOpen, at(4))
        .expect("the opening of two admins");
    // The admins' budget is one pending too: a second opening by admins waits for the first.
    assert!(matches!(
        scene.try_open(Path::QuorumEnrollment, at(5)),
        Err(OpenRefusal::Limit(LimitRefusal::PendingLimit { .. }))
    ));
    let person = member(PERSON);
    assert_eq!(scene.limits.pending_of(person, at(6)), 4);
    for kind in OpenerKind::ALL {
        assert_eq!(
            scene.limits.pending_of_kind(person, kind, at(6)),
            1,
            "{kind:?}"
        );
    }
    assert_eq!(scene.limits.tracked_ledgers(), 4);
    assert_eq!(scene.limits.opened_today(person, at(6)), 4);
}

#[test]
fn the_daily_budget_of_a_kind_of_opener_is_its_own_too() {
    let mut scene = Scene::full();
    // Three helpdesk openings in a day, each one cancelled and closed before the next.
    for n in 0..3u8 {
        let (mut request, _notices) = open(
            &scene.facts,
            &scene.policy,
            Path::EscrowRecovery,
            Opener::helpdesk(member(common::HELPDESK), device(101 + n)),
            at(u64::from(n)),
            &mut scene.limits,
        )
        .unwrap();
        cancel(
            &mut request,
            &scene.facts,
            Canceller::Admin(member(10)),
            at(u64::from(n) + 1),
        )
        .unwrap();
        scene.limits.closed(&request);
    }
    let fourth = open(
        &scene.facts,
        &scene.policy,
        Path::EscrowRecovery,
        Opener::helpdesk(member(common::HELPDESK), device(104)),
        at(10),
        &mut scene.limits,
    );
    assert_eq!(
        fourth.unwrap_err(),
        OpenRefusal::Limit(LimitRefusal::DailyLimit { opened: 3, max: 3 })
    );
    // The admins of the same person still open an escrow opening, and so does the person.
    scene
        .try_open(Path::EscrowOpen, at(11))
        .expect("the admins' budget is untouched");
    scene
        .try_open(Path::RecoveryCode, at(12))
        .expect("the person's budget is untouched");
}

#[test]
fn two_requests_of_one_device_in_one_second_are_two_requests_with_two_ids() {
    // Cancel, then open the same thing in the same second: the id must differ (the request id is
    // the key of the registry), and every replica of the group must compute the same ids.
    let run = || {
        let mut scene = Scene::full();
        let mut ids = Vec::new();
        for _ in 0..3 {
            let mut request = open_for(&mut scene, PERSON, NEW_DEVICE, 100).unwrap();
            ids.push(request.id());
            cancel(
                &mut request,
                &scene.facts,
                Canceller::Admin(member(10)),
                at(100),
            )
            .unwrap();
            scene.limits.closed(&request);
        }
        assert_eq!(scene.limits.peek_seq(member(PERSON), OpenerKind::Person), 3);
        ids
    };
    let ids = run();
    assert_eq!(ids.len(), 3);
    assert_ne!(ids[0], ids[1]);
    assert_ne!(ids[1], ids[2]);
    assert_ne!(ids[0], ids[2]);
    assert_eq!(
        ids,
        run(),
        "the ids are a function of the history, not of entropy"
    );
}

#[test]
fn the_ledger_is_bounded_and_frees_itself_when_it_holds_only_the_idle() {
    let scene = Scene::full();
    let mut limits = OpenLimits::standard();
    let mut last_expiry = at(0);
    for n in 0..MAX_LEDGERS {
        let mut facts = scene.facts.clone();
        facts.person = person_number(n);
        let (request, _notices) = open(
            &facts,
            &scene.policy,
            Path::RecoveryCode,
            Opener::person(device(NEW_DEVICE)),
            at(0),
            &mut limits,
        )
        .unwrap();
        last_expiry = request.expires_at();
    }
    assert_eq!(limits.tracked_ledgers(), MAX_LEDGERS);
    // One more person: the ledger is full of ledgers that are not idle.
    let mut facts = scene.facts.clone();
    facts.person = person_number(MAX_LEDGERS);
    let newcomer = |limits: &mut OpenLimits, now: u64| {
        open(
            &facts,
            &scene.policy,
            Path::RecoveryCode,
            Opener::person(device(NEW_DEVICE)),
            at(now),
            limits,
        )
    };
    assert_eq!(
        newcomer(&mut limits, 10).unwrap_err(),
        OpenRefusal::Limit(LimitRefusal::LedgerFull)
    );
    assert_eq!(
        limits.tracked_ledgers(),
        MAX_LEDGERS,
        "a refusal counts nothing"
    );
    // A person the ledger knows is not a new ledger: the pending limit answers.
    let mut known = scene.facts.clone();
    known.person = person_number(0);
    assert!(matches!(
        open(
            &known,
            &scene.policy,
            Path::RecoveryCode,
            Opener::person(device(101)),
            at(10),
            &mut limits
        ),
        Err(OpenRefusal::Limit(LimitRefusal::PendingLimit { .. }))
    ));
    // Once every request is past its expiry and every opening is a day old, the ledger forgets
    // them by itself and takes the newcomer.
    let later = last_expiry.as_secs() - common::BASE + DAY + 1;
    newcomer(&mut limits, later).expect("the idle ledgers were forgotten");
    assert_eq!(limits.tracked_ledgers(), 1);
}

#[test]
fn the_ledger_forgets_people_with_no_activity() {
    let mut scene = Scene::full();
    for person in 2..12u8 {
        let mut request = open_for(&mut scene, person, 100, 0).unwrap();
        // The facts of THAT person: the veto takes the facts of the person of the request.
        let mut facts = scene.facts.clone();
        facts.person = member(person);
        cancel(&mut request, &facts, Canceller::Admin(member(10)), at(1)).unwrap();
        scene.limits.closed(&request);
    }
    assert_eq!(scene.limits.tracked_ledgers(), 10);
    // A day later nobody has a pending request or a recent opening.
    scene.limits.forget_idle(at(DAY + 1));
    assert_eq!(scene.limits.tracked_ledgers(), 0);
    // A refused opening leaves no entry behind for a person the ledger did not know.
    let mut scene = Scene::business();
    assert!(scene.try_open(Path::PasskeyWrap, at(0)).is_err());
    assert_eq!(scene.limits.tracked_ledgers(), 0);
}
