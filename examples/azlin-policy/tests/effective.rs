//! Checks 5 and 7: the policy of a group down a tree, and the epoch of a change
//! (planning/architecture.md section 5.10 "groups inherit and may only tighten", section 7.2
//! "subgroups inherit and may tighten, never loosen", section 5.11 `policy_epoch`).
//!
//! The guard `policy_effective_skips_clamp` (azlin_seams::mutant!, killers below) turns the clamp
//! off; the guard `policy_epoch_always_bumps` makes every call bump.

mod common;

use azlin_policy::{
    apply, check_tightens, effective, next_epoch, AccountKind, EpochExhausted, Policy, PolicyEpoch,
    PolicyPatch,
};
use azlin_seams::sim::Rng;
use azlin_seams::Span;
use common::{random_patch, random_policy};

/// A patch that sets `credential_ttl`.
fn ttl(span: Span) -> PolicyPatch {
    let mut patch = PolicyPatch::none();
    patch.credential_ttl = Some(span);
    patch
}

#[test]
fn effective_policy_clamps_a_child_looser_than_a_later_tightened_parent() {
    // A team: credentials live 12 h. A group tightens to 6 h, which `check_tightens` accepts.
    let mut account = Policy::defaults(AccountKind::Team);
    assert_eq!(account.credential_ttl, Span::hours(12));
    let group = ttl(Span::hours(6));
    assert_eq!(check_tightens(&account, &apply(&account, &group)), Ok(()));
    assert_eq!(
        effective(&account, &[&group]).credential_ttl,
        Span::hours(6)
    );

    // Later the account tightens to 1 h. The group's patch is as it was (6 h) and is now looser
    // than its parent: the policy that holds is the parent's.
    account = apply(&account, &ttl(Span::hours(1)));
    assert!(check_tightens(&account, &apply(&account, &group)).is_err());
    assert_eq!(
        effective(&account, &[&group]).credential_ttl,
        Span::hours(1),
        "the clamp: a child is never looser than its parent, even when the parent tightened later"
    );

    // Three levels: the middle one tightened to 30 minutes after the bottom one set 45 minutes.
    let top = ttl(Span::hours(1));
    let middle = ttl(Span::minutes(30));
    let bottom = ttl(Span::minutes(45));
    let account = Policy::defaults(AccountKind::Team);
    assert_eq!(
        effective(&account, &[&top, &middle, &bottom]).credential_ttl,
        Span::minutes(30),
        "the bottom patch (45 minutes) is looser than the middle level's 30 minutes"
    );
    // For one field the order of the levels does not matter: every level is clamped to the one
    // above it, so the tightest value on the way down is what holds.
    assert_eq!(
        effective(&account, &[&middle, &bottom, &top]).credential_ttl,
        Span::minutes(30)
    );
    assert_eq!(
        effective(&account, &[&bottom, &middle, &top]).credential_ttl,
        Span::minutes(30)
    );
}

#[test]
fn a_group_inherits_what_it_does_not_set() {
    let mut account = Policy::defaults(AccountKind::Business);
    let mut group = PolicyPatch::none();
    group.credential_ttl = Some(Span::minutes(30));
    group.link_policy.uploads = Some(false);

    let before = effective(&account, &[&group]);
    assert_eq!(before.credential_ttl, Span::minutes(30));
    assert!(!before.link_policy.uploads);
    assert_eq!(before.enroll_hold, account.enroll_hold, "inherited");

    // The account changes a field the group does not set: the group follows, up or down.
    let mut change = PolicyPatch::none();
    change.enroll_hold = Some(Span::hours(96));
    change.max_devices_per_member = Some(3);
    account = apply(&account, &change);
    let after = effective(&account, &[&group]);
    assert_eq!(after.enroll_hold, Span::hours(96));
    assert_eq!(after.max_devices_per_member, 3);
    // What the group set stays.
    assert_eq!(after.credential_ttl, Span::minutes(30));
    assert!(!after.link_policy.uploads);
}

#[test]
fn a_group_cannot_change_a_field_of_the_account_only() {
    let account = Policy::defaults(AccountKind::Business);
    let mut group = PolicyPatch::none();
    group.managed_default = Some(azlin_policy::ManagedDefault::No);
    group.updates = Some(azlin_policy::UpdatePolicy::Auto);
    group.padding = Some(azlin_policy::PaddingPolicy::Full);
    let policy = effective(&account, &[&group]);
    assert_eq!(
        policy, account,
        "the account's managed_default, updates and padding hold"
    );
    // And `check_tightens` refuses the patch at `policy_set` time, three times.
    let refused = check_tightens(&account, &apply(&account, &group)).expect_err("refused");
    assert_eq!(refused.len(), 3);
}

#[test]
fn effective_of_no_patch_is_the_account_policy() {
    for kind in common::KINDS {
        let account = Policy::defaults(kind);
        assert_eq!(effective(&account, &[]), account);
        let none = PolicyPatch::none();
        assert_eq!(effective(&account, &[&none, &none, &none]), account);
    }
}

#[test]
fn effective_is_never_looser_than_any_ancestor_for_5000_seeded_trees_of_depth_5() {
    let mut rng = Rng::new(0xEFFE_0001);
    for tree in 0..5000 {
        let account = random_policy(&mut rng);
        let patches: Vec<PolicyPatch> = (0..5).map(|_| random_patch(&mut rng)).collect();
        let refs: Vec<&PolicyPatch> = patches.iter().collect();

        // The policy at every level of the way down.
        let levels: Vec<Policy> = (0..=5)
            .map(|depth| effective(&account, &refs[..depth]))
            .collect();
        assert_eq!(levels[0], account, "tree {tree}: depth 0 is the account");
        let bottom = &levels[5];

        for (depth, ancestor) in levels.iter().enumerate() {
            assert_eq!(
                check_tightens(ancestor, bottom),
                Ok(()),
                "tree {tree}: the policy at depth 5 is looser than the one at depth {depth}"
            );
        }
        // Each level is its parent with this level's patch applied and clamped; an accepted patch
        // is not changed by the clamp.
        for depth in 0..5 {
            let parent = &levels[depth];
            let child = apply(parent, &patches[depth]);
            if check_tightens(parent, &child).is_ok() {
                assert_eq!(
                    levels[depth + 1],
                    child,
                    "tree {tree}: an accepted patch stands"
                );
            } else {
                assert_ne!(
                    levels[depth + 1],
                    child,
                    "tree {tree}: a refused patch is clamped"
                );
                assert_eq!(check_tightens(parent, &levels[depth + 1]), Ok(()));
            }
        }
    }
}

#[test]
fn policy_epoch_bumps_only_when_the_effective_policy_changes() {
    let old = PolicyEpoch::new(7);
    let account = Policy::defaults(AccountKind::Business);

    // A patch that sets nothing, and a patch that sets what is already there: no bump.
    let none = PolicyPatch::none();
    let before = effective(&account, &[]);
    let after = effective(&account, &[&none]);
    assert_eq!(next_epoch(old, &before, &after), Ok(old));
    let same = ttl(account.credential_ttl);
    assert_eq!(next_epoch(old, &account, &apply(&account, &same)), Ok(old));

    // A change under a group that a tighter parent already clamps: the account asks for 1 h, the
    // group's patch moves from 6 h to 8 h, both looser than the parent: the group's effective policy
    // is the same before and after: no bump.
    let six = ttl(Span::hours(6));
    let eight = ttl(Span::hours(8));
    let before = effective(&account, &[&six]);
    let after = effective(&account, &[&eight]);
    assert_eq!(before.credential_ttl, Span::hours(1));
    assert_eq!(before, after);
    assert_eq!(next_epoch(old, &before, &after), Ok(old));

    // A change that counts: the epoch moves by one.
    let thirty = ttl(Span::minutes(30));
    let tighter = apply(&account, &thirty);
    assert_eq!(next_epoch(old, &account, &tighter), Ok(PolicyEpoch::new(8)));
    // The group's own tightening below the account's policy counts too.
    let before = effective(&account, &[&none]);
    let after = effective(&account, &[&thirty]);
    assert_ne!(before, after);
    assert_eq!(next_epoch(old, &before, &after), Ok(PolicyEpoch::new(8)));
    // And a change back counts again (the epoch is a counter, not a version of the content).
    assert_eq!(
        next_epoch(PolicyEpoch::new(8), &tighter, &account),
        Ok(PolicyEpoch::new(9))
    );
}

#[test]
fn policy_epoch_moves_exactly_when_the_policies_differ_for_1000_seeded_pairs() {
    let mut rng = Rng::new(0xEFFE_0002);
    for round in 0..1000 {
        let a = random_policy(&mut rng);
        // Half of the pairs are equal, half are a patch away.
        let b = if rng.chance(1, 2) {
            a.clone()
        } else {
            apply(&a, &random_patch(&mut rng))
        };
        let old = PolicyEpoch::new(round);
        let next = next_epoch(old, &a, &b).expect("far from the largest number");
        if a == b {
            assert_eq!(next, old, "round {round}");
        } else {
            assert_eq!(next.get(), old.get() + 1, "round {round}");
        }
    }
}

#[test]
fn the_epoch_never_wraps() {
    let largest = PolicyEpoch::new(u32::MAX);
    let account = Policy::defaults(AccountKind::Personal);
    let changed = apply(&account, &ttl(Span::hours(1)));
    // No change: the largest number is not an error.
    assert_eq!(next_epoch(largest, &account, &account), Ok(largest));
    // A change that cannot be counted is an error, never the number 0.
    assert_eq!(next_epoch(largest, &account, &changed), Err(EpochExhausted));
    // One below the largest still counts.
    let below = PolicyEpoch::new(u32::MAX - 1);
    assert_eq!(next_epoch(below, &account, &changed), Ok(largest));
}
