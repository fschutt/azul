//! Check 7: the audience matrix. Five audiences against eight callers, forty cells, each a `#[test]`
//! named `audience_matrix_<audience>_<caller>`; the expected side of every cell is TYPED here from
//! the plan's table (section 5.12: `anyone` with the link; `signed_in` (any Azlin account); `members`
//! (this account, or chosen groups); `people` (named people); `approval` (anyone may ask, and a
//! device of the sharer approves)), not computed by the code under test. Each cell also runs the
//! human step and expects it to agree with the resolve.
//!
//! The callers: an anonymous one; one signed in from another account; a member of the link's own
//! account in a chosen group; a member outside the chosen groups; a named person; an unnamed person;
//! an approved one; an unapproved one.
//!
//! Implements planning/architecture.md section 5.12 (`audience`).

#![allow(dead_code)]

mod common;

use std::collections::BTreeSet;

use azlin_links::counters::{count_open, OpenRefusal};
use azlin_links::resolve::{resolve, Asked, Caller, ResolveCtx, ResolveRefusal};
use azlin_links::{update, Audience, Change, LinkRecord, PublicLinkId};
use azlin_policy::LinkPolicy;
use azlin_seams::{AccountId, ByteCount};
use common::{build, creator, group, groups, member, opts, people, wrap, NOW};

/// No link is blocked.
static NO_BLOCKS: BTreeSet<PublicLinkId> = BTreeSet::new();

/// The audience of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Which {
    Anyone,
    SignedIn,
    Members,
    People,
    Approval,
}

impl Which {
    const ALL: [Which; 5] = [
        Which::Anyone,
        Which::SignedIn,
        Which::Members,
        Which::People,
        Which::Approval,
    ];
}

/// The caller of a cell.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Who {
    Anonymous,
    SignedInFromAnotherAccount,
    MemberInAChosenGroup,
    MemberOutsideTheChosenGroups,
    NamedPerson,
    UnnamedPerson,
    Approved,
    Unapproved,
}

impl Who {
    const ALL: [Who; 8] = [
        Who::Anonymous,
        Who::SignedInFromAnotherAccount,
        Who::MemberInAChosenGroup,
        Who::MemberOutsideTheChosenGroups,
        Who::NamedPerson,
        Who::UnnamedPerson,
        Who::Approved,
        Who::Unapproved,
    ];
}

/// What a cell expects: the side of the answer.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Expect {
    /// The link opens.
    Open,
    /// The caller is asked to sign in.
    SignIn,
    /// The caller is signed in and is not in the audience.
    NotAllowed,
    /// The caller is told to ask for approval.
    NeedsApproval,
}

/// The link of an audience: `members` names the groups 1 and 2, `people` the members 10 and 11,
/// `approval` has approved the member 77.
fn audience_link(which: Which) -> LinkRecord {
    match which {
        Which::Anyone => build(opts(Audience::Anyone)),
        Which::SignedIn => build(opts(Audience::SignedIn)),
        Which::Members => build(opts(groups(&[1, 2]))),
        Which::People => build(opts(people(&[10, 11]))),
        Which::Approval => {
            let mut link = build(opts(Audience::Approval));
            update(
                &mut link,
                Change::AddWrap(wrap(77)),
                &creator(),
                &LinkPolicy::allow_all(),
                NOW,
            )
            .expect("the sender approves the member 77");
            link
        }
    }
}

/// The caller of a cell.
fn caller(who: Who) -> Caller {
    let other_account = AccountId::from_bytes([200; 16]);
    let own_account = AccountId::from_bytes([1; 16]);
    match who {
        Who::Anonymous => Caller::Anonymous,
        Who::SignedInFromAnotherAccount => Caller::SignedIn {
            account: other_account,
            member: member(90),
        },
        Who::MemberInAChosenGroup => Caller::Member {
            account: own_account,
            member: member(20),
            groups: [group(1)].into_iter().collect(),
        },
        Who::MemberOutsideTheChosenGroups => Caller::Member {
            account: own_account,
            member: member(21),
            groups: [group(7)].into_iter().collect(),
        },
        Who::NamedPerson => Caller::SignedIn {
            account: other_account,
            member: member(10),
        },
        Who::UnnamedPerson => Caller::SignedIn {
            account: other_account,
            member: member(99),
        },
        Who::Approved => Caller::SignedIn {
            account: other_account,
            member: member(77),
        },
        Who::Unapproved => Caller::SignedIn {
            account: other_account,
            member: member(88),
        },
    }
}

/// The context of a request for `link` by `caller` at `NOW`.
fn ctx_for(link: &LinkRecord, caller: Caller, human_step: bool) -> ResolveCtx<'static> {
    ResolveCtx {
        asked: Asked::Id(*link.id()),
        now: NOW,
        caller,
        device: None,
        blocks: &NO_BLOCKS,
        links_disabled: false,
        served_today: ByteCount::ZERO,
        daily_cap: ByteCount::gib(100),
        password_verified: false,
        human_step,
    }
}

/// Runs one cell: the resolve answers on the expected side, and the human step agrees with it.
fn run(which: Which, who: Who, expect: Expect) {
    let link = audience_link(which);
    let refusal = resolve(&link, &ctx_for(&link, caller(who), false)).err();
    let side = matches!(
        (&expect, &refusal),
        (Expect::Open, None)
            | (Expect::SignIn, Some(ResolveRefusal::SignInRequired))
            | (Expect::NotAllowed, Some(ResolveRefusal::NotAllowed))
            | (
                Expect::NeedsApproval,
                Some(ResolveRefusal::NeedsApproval { .. })
            )
    );
    assert!(
        side,
        "{which:?} x {who:?}: expected {expect:?}, got {refusal:?}"
    );

    // The human step asks the same question and agrees.
    let mut counted = link.clone();
    let human = ctx_for(&counted, caller(who), true);
    let opened = count_open(&mut counted, &human, false);
    match refusal {
        None => assert!(opened.is_ok(), "{which:?} x {who:?}: the open is counted"),
        Some(refusal) => {
            assert_eq!(
                opened.unwrap_err(),
                OpenRefusal::Link(refusal),
                "{which:?} x {who:?}: the human step refuses as the resolve does"
            );
            assert_eq!(
                counted.state_hash(),
                link.state_hash(),
                "{which:?} x {who:?}: a refused open changes nothing"
            );
        }
    }
}

/// The forty cells: the name of the test, the audience, the caller and the expected side.
macro_rules! matrix {
    ($($name:ident: $which:ident, $who:ident => $expect:ident;)+) => {
        /// The cells, as data.
        const CELLS: &[(Which, Who, Expect)] = &[
            $((Which::$which, Who::$who, Expect::$expect)),+
        ];

        $(
            #[test]
            fn $name() {
                run(Which::$which, Who::$who, Expect::$expect);
            }
        )+
    };
}

matrix! {
    audience_matrix_anyone_anonymous: Anyone, Anonymous => Open;
    audience_matrix_anyone_signed_in_from_another_account: Anyone, SignedInFromAnotherAccount => Open;
    audience_matrix_anyone_member_in_a_chosen_group: Anyone, MemberInAChosenGroup => Open;
    audience_matrix_anyone_member_outside_the_chosen_groups: Anyone, MemberOutsideTheChosenGroups => Open;
    audience_matrix_anyone_named_person: Anyone, NamedPerson => Open;
    audience_matrix_anyone_unnamed_person: Anyone, UnnamedPerson => Open;
    audience_matrix_anyone_approved: Anyone, Approved => Open;
    audience_matrix_anyone_unapproved: Anyone, Unapproved => Open;

    audience_matrix_signed_in_anonymous: SignedIn, Anonymous => SignIn;
    audience_matrix_signed_in_signed_in_from_another_account: SignedIn, SignedInFromAnotherAccount => Open;
    audience_matrix_signed_in_member_in_a_chosen_group: SignedIn, MemberInAChosenGroup => Open;
    audience_matrix_signed_in_member_outside_the_chosen_groups: SignedIn, MemberOutsideTheChosenGroups => Open;
    audience_matrix_signed_in_named_person: SignedIn, NamedPerson => Open;
    audience_matrix_signed_in_unnamed_person: SignedIn, UnnamedPerson => Open;
    audience_matrix_signed_in_approved: SignedIn, Approved => Open;
    audience_matrix_signed_in_unapproved: SignedIn, Unapproved => Open;

    audience_matrix_members_anonymous: Members, Anonymous => SignIn;
    audience_matrix_members_signed_in_from_another_account: Members, SignedInFromAnotherAccount => NotAllowed;
    audience_matrix_members_member_in_a_chosen_group: Members, MemberInAChosenGroup => Open;
    audience_matrix_members_member_outside_the_chosen_groups: Members, MemberOutsideTheChosenGroups => NotAllowed;
    audience_matrix_members_named_person: Members, NamedPerson => NotAllowed;
    audience_matrix_members_unnamed_person: Members, UnnamedPerson => NotAllowed;
    audience_matrix_members_approved: Members, Approved => NotAllowed;
    audience_matrix_members_unapproved: Members, Unapproved => NotAllowed;

    audience_matrix_people_anonymous: People, Anonymous => SignIn;
    audience_matrix_people_signed_in_from_another_account: People, SignedInFromAnotherAccount => NotAllowed;
    audience_matrix_people_member_in_a_chosen_group: People, MemberInAChosenGroup => NotAllowed;
    audience_matrix_people_member_outside_the_chosen_groups: People, MemberOutsideTheChosenGroups => NotAllowed;
    audience_matrix_people_named_person: People, NamedPerson => Open;
    audience_matrix_people_unnamed_person: People, UnnamedPerson => NotAllowed;
    audience_matrix_people_approved: People, Approved => NotAllowed;
    audience_matrix_people_unapproved: People, Unapproved => NotAllowed;

    audience_matrix_approval_anonymous: Approval, Anonymous => NeedsApproval;
    audience_matrix_approval_signed_in_from_another_account: Approval, SignedInFromAnotherAccount => NeedsApproval;
    audience_matrix_approval_member_in_a_chosen_group: Approval, MemberInAChosenGroup => NeedsApproval;
    audience_matrix_approval_member_outside_the_chosen_groups: Approval, MemberOutsideTheChosenGroups => NeedsApproval;
    audience_matrix_approval_named_person: Approval, NamedPerson => NeedsApproval;
    audience_matrix_approval_unnamed_person: Approval, UnnamedPerson => NeedsApproval;
    audience_matrix_approval_approved: Approval, Approved => Open;
    audience_matrix_approval_unapproved: Approval, Unapproved => NeedsApproval;
}

#[test]
fn the_matrix_has_one_cell_for_each_audience_and_caller() {
    assert_eq!(CELLS.len(), 40, "five audiences x eight callers");
    for which in Which::ALL {
        for who in Who::ALL {
            let cells = CELLS
                .iter()
                .filter(|(w, c, _)| *w == which && *c == who)
                .count();
            assert_eq!(cells, 1, "{which:?} x {who:?}");
        }
    }
}

#[test]
fn members_with_no_chosen_group_is_the_whole_account() {
    // ("`members` (this account, or chosen groups)": no chosen group means the whole account.)
    let link = build(opts(groups(&[])));
    for groups_of_the_member in [&[][..], &[3][..], &[1, 2][..]] {
        let member_of_the_account = Caller::Member {
            account: AccountId::from_bytes([1; 16]),
            member: member(20),
            groups: groups_of_the_member.iter().map(|g| group(*g)).collect(),
        };
        assert!(
            resolve(&link, &ctx_for(&link, member_of_the_account, false)).is_ok(),
            "every member of the account opens it"
        );
    }
    assert_eq!(
        resolve(
            &link,
            &ctx_for(&link, caller(Who::SignedInFromAnotherAccount), false)
        )
        .unwrap_err(),
        ResolveRefusal::NotAllowed,
        "a member of another account does not"
    );
    assert_eq!(
        resolve(&link, &ctx_for(&link, Caller::Anonymous, false)).unwrap_err(),
        ResolveRefusal::SignInRequired
    );
}

#[test]
fn a_named_person_who_is_a_member_of_the_account_opens_a_people_link() {
    let link = build(opts(people(&[10, 11])));
    let insider = Caller::Member {
        account: AccountId::from_bytes([1; 16]),
        member: member(11),
        groups: BTreeSet::new(),
    };
    assert!(resolve(&link, &ctx_for(&link, insider, false)).is_ok());
}

#[test]
fn a_member_of_a_chosen_group_among_several_groups_opens_the_link() {
    let link = build(opts(groups(&[1, 2])));
    let caller = Caller::Member {
        account: AccountId::from_bytes([1; 16]),
        member: member(20),
        groups: [group(5), group(2), group(9)].into_iter().collect(),
    };
    assert!(resolve(&link, &ctx_for(&link, caller, false)).is_ok());
}

#[test]
fn approving_a_member_approves_no_other_member_of_the_account() {
    // Approving the member 77 approves that member: another caller with the same account is not.
    let link = audience_link(Which::Approval);
    let same_account_other_member = Caller::SignedIn {
        account: AccountId::from_bytes([200; 16]),
        member: member(78),
    };
    assert!(matches!(
        resolve(&link, &ctx_for(&link, same_account_other_member, false)).unwrap_err(),
        ResolveRefusal::NeedsApproval { .. }
    ));
}
