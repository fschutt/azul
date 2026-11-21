//! Who may open a link: the `audience` option of planning/architecture.md section 5.12.
//!
//! | audience | who | how the share key reaches them | strength |
//! |---|---|---|---|
//! | `anyone` | anyone with the link | in the URL's fragment | key |
//! | `signed_in` | any Azlin account | in the URL's fragment | server |
//! | `members` | this account, or chosen groups | in the URL's fragment | server |
//! | `people` | named people | wrapped to each one's member key: "a forwarded URL opens nothing" | key |
//! | `approval` | anyone may ask, and a device of the sharer approves | wrapped to the requester's key on approval | key |
//!
//! The classes of the table are `azlin_policy::AudienceClass` (the policy allows a class or not);
//! this type adds who is named inside a class: the groups of `members` and the people of `people`.
//! Both are sets (`BTreeSet`): one canonical order, no duplicate, so two replicas that apply the same
//! calls hold the same state.
//!
//! Three answers of this module are readings the plan does not spell out (drift register of lane
//! W1-S16c1): "named audiences" (`edit`, `slug`) are `people` only; "signed-in audiences" (`comment`,
//! `open_log`) are every audience but `anyone`; and an audience changes without a rotation only
//! between `signed_in` and `members` (and between two lists of groups).
//!
//! Implements planning/architecture.md section 5.12 (`audience`, "Changing a link").

use std::collections::BTreeSet;

use azlin_policy::AudienceClass;
use azlin_seams::{GroupId, MemberId};

use crate::strength::Strength;

/// The most groups a `members` audience may name. The plan gives no limit (1.4 rule 4: everything
/// has a limit, stated up front): a link for "chosen groups" names a handful, and every group is a
/// check at `link_resolve`. An estimate (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.12 (`members`).
///
/// Caller: `validate` (`TooManyGroups`).
pub const MAX_GROUPS: usize = 100;

/// The most people a `people` audience may name: one wrap of the share key for each, kept in the
/// link's row. An estimate (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits) and section 5.12 (`people`).
///
/// Caller: `validate` (`TooManyPeople`).
pub const MAX_PEOPLE: usize = 100;

/// Who may open a link.
///
/// Implements planning/architecture.md section 5.12 (`audience`: `anyone`, `signed_in`, `members`,
/// `people`, `approval`).
///
/// Caller: `LinkOptions::audience`, `validate`, `apply_policy`, `update`, `rotate`;
/// Seam: S16c2 (`resolve` checks the caller against it), S16d (the share dialog).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Audience {
    /// Anyone with the link.
    Anyone,
    /// Any Azlin account.
    SignedIn,
    /// Members of this account: all of them when `groups` is empty, else the members of the chosen
    /// groups.
    Members {
        /// The chosen groups; empty means the whole account.
        groups: BTreeSet<GroupId>,
    },
    /// Named people: the share key is wrapped to each one's member key.
    People {
        /// The named people, by member id (at least one, at most [`MAX_PEOPLE`]).
        people: BTreeSet<MemberId>,
    },
    /// Anyone may ask, and a device of the sharer approves (the link's paired request carries the
    /// ask).
    Approval,
}

impl Audience {
    /// The class of this audience: what a `link_policy` allows or forbids.
    ///
    /// Implements planning/architecture.md section 5.10 (`link_policy`: the audiences allowed).
    pub fn class(&self) -> AudienceClass {
        match self {
            Audience::Anyone => AudienceClass::Anyone,
            Audience::SignedIn => AudienceClass::SignedIn,
            Audience::Members { .. } => AudienceClass::Members,
            Audience::People { .. } => AudienceClass::People,
            Audience::Approval => AudienceClass::Approval,
        }
    }

    /// The strength of this audience: "key for `anyone`, `people` and `approval`; server for
    /// `signed_in` and `members`".
    ///
    /// Implements planning/architecture.md section 5.12 (the strength cell of `audience`).
    pub fn strength(&self) -> Strength {
        match self {
            Audience::Anyone | Audience::People { .. } | Audience::Approval => Strength::Key,
            Audience::SignedIn | Audience::Members { .. } => Strength::Server,
        }
    }

    /// Whether the audience is signed in: the caller of `link_resolve` is an Azlin account. "Signed-in
    /// audiences" (`comment`, `open_log`) are every audience but `anyone` (the brief's reading,
    /// drift register of lane W1-S16c1).
    ///
    /// Implements planning/architecture.md section 5.12 (`comment`, `open_log`: "signed-in
    /// audiences").
    pub fn is_signed_in(&self) -> bool {
        !matches!(self, Audience::Anyone)
    }

    /// Whether the audience is named: `people` only (the brief's reading of "named audiences" for
    /// `edit` and `slug`, drift register of lane W1-S16c1).
    ///
    /// Implements planning/architecture.md section 5.12 (`edit`, `slug`: "named audience").
    pub fn is_named(&self) -> bool {
        matches!(self, Audience::People { .. })
    }

    /// Whether the share key is wrapped to member keys instead of travelling in the URL's fragment:
    /// `people` and `approval`.
    ///
    /// Implements planning/architecture.md section 5.12 (`people`: "the share key is wrapped to each
    /// one's member key, so a forwarded URL opens nothing"; `approval`: "approving wraps the share
    /// key to that key").
    pub fn key_is_wrapped(&self) -> bool {
        matches!(self, Audience::People { .. } | Audience::Approval)
    }

    /// Whether the audience needs the link's paired request: `approval` (the recipient's sealed ask
    /// travels through it).
    ///
    /// Implements planning/architecture.md section 5.12 ("Asking for access").
    pub fn needs_paired_request(&self) -> bool {
        matches!(self, Audience::Approval)
    }

    /// Whether a link may change from this audience to `to` without a rotation. Moving between
    /// audiences of the same key strength AND the same way of handing out the key (both in the
    /// fragment) changes a server-side filter only: `signed_in` and `members`, and one list of
    /// groups for another. Every other change ("changing audience across key strength", and any
    /// change between the fragment and the wraps) changes who can decrypt, and "who can decrypt
    /// changes only by rotating". Two equal audiences are no change at all (the caller checks).
    ///
    /// Implements planning/architecture.md section 5.12 ("Changing a link": "server options change at
    /// any time; who can decrypt changes only by rotating").
    ///
    /// Caller: `update` (`Change::Audience`).
    pub fn changes_without_rotation(&self, to: &Audience) -> bool {
        matches!(
            (self, to),
            (Audience::Members { .. }, Audience::Members { .. })
                | (Audience::SignedIn, Audience::Members { .. })
                | (Audience::Members { .. }, Audience::SignedIn)
        )
    }
}
