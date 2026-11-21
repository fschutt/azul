//! What a space is and how its history is kept: the `kind` and `history` columns of `reg_space`
//! (planning/architecture.md section 5.4 "Spaces"; build unit S16, lane W1-S16b).
//!
//! Both are closed vocabularies with strict words (`FromStr` accepts exactly the one spelling of the
//! registry's column and nothing else, an unknown word is an error and never a default), `Copy`, `Ord`
//! (declaration order: for maps and sorting only, NOT a privilege order) and `Hash`.

/// The kind of a space. The kind decides who may create it (`admit`): the account's one `root` space
/// only at account creation, a `private_member` space only in a family account, a `system_audit` space
/// only by an admin and only when `audit_level` leaves `none`, every other kind by an admin or, where
/// the account's policy lets members, by a member.
///
/// Implements planning/architecture.md section 5.4 (`reg_space.kind`: `root`, `shared`, `team`,
/// `private_member` (family), `inbox`, `conversation`, `system_audit`) and section 5.9 (members create
/// conversation spaces where policy allows).
///
/// Caller: `SpaceOp::Create`, `SpaceState::kind`, `admit` (the creation rules); S9 (`reg_space.kind`,
/// through the SPACES-APPLY glue).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum SpaceKind {
    /// The account's own root space: exactly one, made at account creation.
    Root,
    /// A shared space: a folder (or drive) several members work in.
    Shared,
    /// A team's space, granted to a group.
    Team,
    /// A member's private space in a family account.
    PrivateMember,
    /// An inbox: a mailbox or a request inbox.
    Inbox,
    /// A conversation: a direct message, a group chat or a channel (every conversation is a space).
    Conversation,
    /// The audit space of the account (made when `audit_level` leaves `none`).
    SystemAudit,
}

words!(
    SpaceKind,
    "space kind",
    [
        Root => "root",
        Shared => "shared",
        Team => "team",
        PrivateMember => "private_member",
        Inbox => "inbox",
        Conversation => "conversation",
        SystemAudit => "system_audit",
    ]
);

/// How a space's history is kept for the members who join it later: `full` (joiners get the epoch
/// chain, so they read what was written before they came) or `forward` (the key rotates on every add
/// and no chain wrap is made, so a joiner reads only what is written after the rotation that follows
/// its grant).
///
/// Implements planning/architecture.md section 5.4 (`reg_space.history`: "`full` (joiners get the epoch
/// chain) or `forward` (rotate on add, no chain for joiners)") and section 6.5 (`history = full` or
/// `forward` per conversation).
///
/// Caller: `SpaceOp::Create`, `SpaceOp::History`, `apply` (a grant in a forward space makes a rekey
/// due), `chain_visible_to`; S9 (`reg_space.history`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum HistoryMode {
    /// Joiners get the epoch chain: every epoch's wrap chain is visible to them.
    Full,
    /// Rotate on add: a joiner gets only the epochs from the rotation that follows its grant.
    Forward,
}

words!(
    HistoryMode,
    "history mode",
    [Full => "full", Forward => "forward"]
);
