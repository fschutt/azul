//! Check 3 of the brief: near misses. Each kind is a generator mode: a valid entry changed in exactly one
//! place (one role too low, one group off, one person short, one second too late ...). In each run of the
//! mode at least 100 cases must be generated, and on both sides every one of them is a REFUSAL of the right
//! class (planning/architecture.md section 12 E1: "no entry is accepted unless its signer may sign it; a
//! quorum counts distinct people, never one person's devices").
//!
//! A test named here is also the killer of a production guard: `e1_catches_every_teamlog_mutant` (in
//! `e1_hygiene.rs`) lists, for each of the twelve guards of `azlin-teamlog`, the test of this file that must
//! fail when the guard is turned off.

mod common;

use azlin_escalate_e1::compare::{NearStats, Plan};
use azlin_escalate_e1::gen::NearMiss;

/// Runs the near miss until it has 100 cases (at most 1,200 seeds): no disagreement may occur, the
/// production must refuse every case, and it must refuse with the intended class in most of them.
fn check(nm: NearMiss) {
    let mut total = NearStats::default();
    let mut start = 0u64;
    while total.cases < 100 && start < 1_200 {
        let run = common::run_seeds(start..start + 100, common::STEPS, Plan::Focus(nm));
        assert!(
            run.failures.is_empty(),
            "near miss {}: {}",
            nm.name(),
            common::fail_report(&run)
        );
        if let Some(n) = run.stats.near.get(&nm) {
            total.cases += n.cases;
            total.both_refused += n.both_refused;
            total.exact_class += n.exact_class;
        }
        start += 100;
    }
    assert!(
        total.cases >= 100,
        "near miss {} reached only {} cases in {} seeds: the generator does not get there",
        nm.name(),
        total.cases,
        start
    );
    assert_eq!(
        total.both_refused,
        total.cases,
        "near miss {}: an entry that is wrong in one place was admitted",
        nm.name()
    );
    println!(
        "near miss {}: {} cases, {} with exactly the intended class {:?}",
        nm.name(),
        total.cases,
        total.exact_class,
        nm.expected()
    );
    assert!(
        total.exact_class * 10 >= total.cases * 6,
        "near miss {}: only {} of {} cases were refused with the intended class {:?}",
        nm.name(),
        total.exact_class,
        total.cases,
        nm.expected()
    );
}

macro_rules! near_misses {
    ($($name:ident => $kind:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                check(NearMiss::$kind);
            }
        )+
    };
}

near_misses! {
    near_miss_signer_one_role_too_low => SignerOneRoleTooLow,
    near_miss_signer_scope_off_by_one_group => SignerScopeOffByOneGroup,
    near_miss_quorum_short_by_one_person => QuorumShortByOnePerson,
    near_miss_quorum_met_by_two_devices_of_one_person => QuorumMetByTwoDevicesOfOnePerson,
    near_miss_cosig_by_a_suspended_admin => CosigByASuspendedAdmin,
    near_miss_cosig_over_another_entry_hash => CosigOverAnotherEntryHash,
    near_miss_wrong_prev_or_seq => WrongPrevOrSeq,
    near_miss_delegation_expired_by_one_second => DelegationExpiredByOneSecond,
    near_miss_delegate_op_outside_the_named_set => DelegateOpOutsideTheNamedSet,
    near_miss_delegate_names_a_role_or_wrap_op => DelegateNamesARoleOrWrapOp,
    near_miss_group_move_into_own_subtree => GroupMoveIntoOwnSubtree,
    near_miss_last_owner_demoted_removed_or_suspended => LastOwnerDemotedRemovedOrSuspended,
    near_miss_self_role_change => SelfRoleChange,
    near_miss_helpdesk_adds_a_device_alone => HelpdeskAddsADeviceAlone,
    near_miss_entitlement_signed_by_a_device => EntitlementSignedByADevice,
    near_miss_app_op_by_a_guest => AppOpByAGuest,
    near_miss_pending_device_signs => PendingDeviceSigns,
    near_miss_policy_set_that_loosens_a_group => PolicySetThatLoosensAGroup,
    near_miss_suspended_member_signs => SuspendedMemberSigns,
    near_miss_revoked_device_signs => RevokedDeviceSigns,
    near_miss_forged_signature => ForgedSignature,
    near_miss_corrupt_copy => CorruptCopy,
    near_miss_stamp_backwards => StampBackwards,
    near_miss_epoch_below_chain => EpochBelowChain,
    near_miss_wrong_account => WrongAccount,
    near_miss_unknown_wire_op => UnknownWireOp,
    near_miss_escrow_open_without_assertion => EscrowOpenWithoutAssertion,
    near_miss_join_with_wrong_secret => JoinWithWrongSecret,
    near_miss_private_to_managed_without_consent => PrivateToManagedWithoutConsent,
    near_miss_device_add_quorum_under_self_policy => DeviceAddQuorumUnderSelfPolicy,
    near_miss_group_admin_pulls_an_admin_into_scope => GroupAdminPullsAnAdminIntoScope,
    near_miss_admin_enrolls_a_device_for_the_owner => AdminEnrollsADeviceForTheOwner,
    near_miss_delegate_suspends_an_owner => DelegateSuspendsAnOwner,
    near_miss_entitlement_not_newer => EntitlementNotNewer,
    near_miss_reused_id => ReusedId,
    near_miss_root_group_moves => RootGroupMoves,
}
