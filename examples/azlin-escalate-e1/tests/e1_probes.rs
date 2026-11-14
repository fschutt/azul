//! Probes: attacks on the engine that go beyond the cells of the authority table (planning/architecture.md
//! section 12 E1; ARCH29 9.13 X-D-54). A probe that fails is a finding; a probe marked `#[ignore]` only
//! prints what the production does in a corner the plan does not decide (run with
//! `-- --ignored --nocapture`).

use std::collections::BTreeSet;

use azlin_escalate_e1::model::{
    AppId, Class, Dev, EpochSpec, Grp, Key, MKind, MOp, Mem, OpKind, Role, MAX_GROUP_DEPTH,
};
use azlin_escalate_e1::stories::{Story, World};
use azlin_seams::Stamp;

type R = Result<(), String>;

fn set<T: Ord + Copy>(items: &[T]) -> BTreeSet<T> {
    items.iter().copied().collect()
}

/// A plain member signs an ordinary app op that claims the largest restore epoch there is. 6.10 V3: "a
/// restore epoch the metadata group signs"; 5.13: the only way back is a restore the metadata group
/// announces. An entry of a device must not be able to move the epoch: a device that has not heard of the
/// jump signs with the old epoch and is refused (BelowRestoreEpoch), and the epoch can never be raised by one
/// again (`RestoreEpoch::next` of `u64::MAX` is `None`), so the next restore cannot be announced. The
/// production admits the entry (finding X-DELTA-W1S47A-3, class BUG).
#[test]
#[ignore = "X-DELTA-W1S47A-3: any member's entry can raise the chain's restore epoch to u64::MAX"]
fn story_member_claims_a_restore_epoch_from_the_future() -> R {
    let mut w = World::team()?;
    let (m, o) = (w.member, w.owner);
    let mut e = w.s.e(
        m.dev,
        MOp::App {
            name: AppId(1),
            body_len: 8,
        },
    );
    e.epoch = EpochSpec::Far;
    w.s.refuse(
        "a member's app op claims the restore epoch u64::MAX (only an announced restore moves the epoch)",
        Class::Chain,
        e,
    )?;
    let e = w.s.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Guest,
            scope: None,
        },
    );
    w.s.admit("the owner's next entry is admitted", e)?;
    Ok(())
}

/// Prints what follows an entry that raised the epoch: the next honest entries.
#[test]
#[ignore = "probe: prints what follows an entry with a far restore epoch"]
fn probe_far_epoch_consequences() {
    let mut w = World::team().expect("the world");
    let (m, o) = (w.member, w.owner);
    let mut e = w.s.e(
        m.dev,
        MOp::App {
            name: AppId(1),
            body_len: 8,
        },
    );
    e.epoch = EpochSpec::Far;
    w.s.tick(1);
    let r1 = w.s.rig.step(&e, w.s.now);
    println!(
        "far epoch entry: model {:?}, production {:?}, replay {:?}, applied {}",
        r1.model, r1.prod, r1.replay, r1.applied
    );
    for i in 0..3 {
        let mut e = w.s.e(
            o.dev,
            MOp::MemberRole {
                member: m.mem,
                role: if i == 1 { Role::Guest } else { Role::Member },
                scope: None,
            },
        );
        if i == 0 {
            // the owner's device has not heard of the jump: it still signs with the epoch it knew (0)
            e.epoch = EpochSpec::At(0);
        }
        w.s.tick(1);
        let r = w.s.rig.step(&e, w.s.now);
        println!(
            "the owner's next entry {i}: model {:?}, production {:?} ({:?}), replay {:?}",
            r.model, r.prod, r.prod_refusal, r.replay
        );
    }
}

/// An invite secret is spent by the entry that carries it (the entry is logged and public). Somebody who
/// reads the log replays the secret for a member who joined, with a device of his own. 5.11: the secret is
/// "spent"; "the invite unused" (table v0).
#[test]
fn story_a_spent_invite_secret_cannot_join_a_second_device() -> R {
    let mut w = World::team()?;
    let (o, m) = (w.owner, w.member);
    let secret_of_member = 1004; // the world's invitation of `member`
    let e = w.s.e(
        Dev(777),
        MOp::MemberJoin {
            member: m.mem,
            device: Dev(777),
            secret: secret_of_member,
        },
    );
    let e = Story::with_cosigs(e, &[o.dev]);
    w.s.refuse(
        "the logged secret of a member who already joined is replayed with a new device",
        Class::InviteInvalid,
        e,
    )?;
    // and an invitation that is still open is spent by the first join
    let e = w.s.e(
        o.dev,
        MOp::MemberInvite {
            member: Mem(70),
            kind: MKind::Person,
            role: Role::Member,
            secret: 7070,
            groups: BTreeSet::new(),
            app: None,
        },
    );
    w.s.admit("the owner invites a new member", e)?;
    let join = |w: &mut World, dev: u16| {
        let e = w.s.e(
            Dev(dev),
            MOp::MemberJoin {
                member: Mem(70),
                device: Dev(dev),
                secret: 7070,
            },
        );
        Story::with_cosigs(e, &[o.dev])
    };
    let first = join(&mut w, 701);
    w.s.admit("the invitee joins with the secret", first)?;
    let second = join(&mut w, 702);
    w.s.refuse(
        "a second device joins the same member with the same secret",
        Class::InviteInvalid,
        second,
    )?;
    Ok(())
}

/// Everything has a limit (1.4 rule 4): the group tree is at most `MAX_GROUP_DEPTH + 1` levels deep, when a
/// group is made and when a subtree is moved.
#[test]
fn story_the_group_tree_has_a_depth_limit() -> R {
    let mut w = World::team()?;
    let a = w.admin_a;
    let mut parent = w.other; // depth 0
    for i in 0..MAX_GROUP_DEPTH as u16 {
        let g = Grp(100 + i);
        let e = w.s.e(
            a.dev,
            MOp::GroupCreate {
                group: g,
                parent: Some(parent),
            },
        );
        w.s.admit("a group one level deeper", e)?;
        parent = g;
    }
    let e = w.s.e(
        a.dev,
        MOp::GroupCreate {
            group: Grp(200),
            parent: Some(parent),
        },
    );
    w.s.refuse("one level too deep", Class::Invalid, e)?;
    // a small subtree elsewhere: 300 with a child 301
    for (g, p) in [(Grp(300), w.ops), (Grp(301), Grp(300))] {
        let e = w.s.e(
            a.dev,
            MOp::GroupCreate {
                group: g,
                parent: Some(p),
            },
        );
        w.s.admit("a small subtree", e)?;
    }
    let e = w.s.e(
        a.dev,
        MOp::GroupMove {
            group: Grp(300),
            new_parent: parent,
        },
    );
    w.s.refuse(
        "moving a subtree under the deepest group makes the tree too deep",
        Class::Invalid,
        e,
    )?;
    Ok(())
}

/// A delegation that names an expiry is live until that stamp and over from it (reading: the plan gives a
/// delegation no expiry, table v0 neither; lane S17c's `expires` is "the stamp from which it is over", and
/// the model reads it the same way: drift row X-DELTA-W1S47A-10).
#[test]
fn story_a_delegation_ends_at_its_stamp() -> R {
    let mut w = World::team()?;
    let o = w.owner;
    let key = Key(1);
    w.s.tick(1);
    let expires = w.s.now.as_secs() + 600;
    let e = w.s.e(
        o.dev,
        MOp::Delegate {
            key,
            ops: set(&[OpKind::MemberInvite]),
            expires: Some(Stamp::from_secs(expires)),
        },
    );
    w.s.admit(
        "the owner delegates `member_invite` to key 1 until a stamp",
        e,
    )?;
    let invite = |w: &mut World, n: u16| {
        w.s.e_delegate(
            key,
            MOp::MemberInvite {
                member: Mem(80 + n),
                kind: MKind::Person,
                role: Role::Member,
                secret: u32::from(n),
                groups: BTreeSet::new(),
                app: None,
            },
        )
    };
    w.s.set_clock(expires - 2);
    let e = invite(&mut w, 1);
    w.s.admit("one second before the stamp the key invites a member", e)?;
    w.s.set_clock(expires - 1);
    let e = invite(&mut w, 2);
    w.s.refuse(
        "at the stamp the delegation is over",
        Class::DelegationExpired,
        e,
    )?;
    let e = w.s.e(
        o.dev,
        MOp::Delegate {
            key,
            ops: set(&[OpKind::MemberInvite]),
            expires: None,
        },
    );
    w.s.admit("the owner renews it without an end", e)?;
    let e = invite(&mut w, 3);
    w.s.admit("the key invites again", e)?;
    Ok(())
}

/// Prints what the production does with the same entry delivered twice: to the metadata group, and in the
/// log a device replays.
#[test]
#[ignore = "probe: prints what a duplicate delivery does"]
fn probe_a_duplicate_in_the_log() {
    let mut w = World::team().expect("the world");
    let (o, m) = (w.owner, w.member);
    let e = w.s.e(
        o.dev,
        MOp::MemberRole {
            member: m.mem,
            role: Role::Helpdesk,
            scope: Some(w.eng),
        },
    );
    w.s.admit("E", e).expect("E is admitted");
    let (raw, at) = w.s.rig.log.last().cloned().expect("the log");
    w.s.tick(1);
    let again = w.s.probe_production(&raw, w.s.now);
    println!("the group is offered E again at its own position: {again:?}");
    let mut entries: Vec<_> = w.s.rig.log.iter().map(|(e, _)| e.clone()).collect();
    let mut stamps: Vec<Stamp> = w.s.rig.log.iter().map(|(_, s)| *s).collect();
    println!(
        "the log replays: {:?}",
        w.s.rig.replay_log(&entries, &stamps).map(|_| "ok")
    );
    entries.push(raw);
    stamps.push(at);
    println!(
        "the log with E twice replays: {:?}",
        w.s.rig.replay_log(&entries, &stamps).map(|_| "ok")
    );
}

/// Prints how the stamps a device replays with decide what it accepts: the leader's stamps are an input of
/// `replay`, and nothing in the entry binds them. A delegate key that is live at the true stamp of its
/// entry is expired at a later stamp, and the device stops at that entry.
#[test]
#[ignore = "probe: prints what other stamps do to a device's replay"]
fn probe_the_stamps_a_device_replays_with_decide_what_it_accepts() {
    let mut w = World::team().expect("the world");
    let (o, m) = (w.owner, w.member);
    let key = Key(1);
    w.s.tick(1);
    let expires = Stamp::from_secs(w.s.now.as_secs() + 600);
    let e = w.s.e(
        o.dev,
        MOp::Delegate {
            key,
            ops: set(&[OpKind::MemberSuspend]),
            expires: Some(expires),
        },
    );
    w.s.admit("delegate", e).expect("delegate");
    w.s.tick(60);
    let e = w.s.e_delegate(key, MOp::MemberSuspend { member: m.mem });
    w.s.admit("suspend by the delegate, 60 s later", e)
        .expect("suspend");
    let entries: Vec<_> = w.s.rig.log.iter().map(|(e, _)| e.clone()).collect();
    let stamps: Vec<Stamp> = w.s.rig.log.iter().map(|(_, s)| *s).collect();
    println!(
        "true stamps: {:?}",
        w.s.rig.replay_log(&entries, &stamps).map(|_| "ok")
    );
    let later: Vec<Stamp> = stamps
        .iter()
        .enumerate()
        .map(|(i, s)| {
            if i + 1 == stamps.len() {
                Stamp::from_secs(s.as_secs() + 7200)
            } else {
                *s
            }
        })
        .collect();
    println!(
        "the last entry's stamp two hours later: {:?}",
        w.s.rig.replay_log(&entries, &later).map(|_| "ok")
    );
}
