//! Resolving a link: what the nodes decide when `link_resolve` arrives (planning/architecture.md
//! section 5.12 "Resolving": "`link_resolve` checks every server option (dates, counts, devices,
//! `paused`, blocks, the audience), ... and returns the manifest object and a **link credential**").
//!
//! `resolve` is a read of the link's row and COUNTS NOTHING: it takes the record by shared reference,
//! so the compiler holds the rule of section 5.12 "Scanners and previews" ("an open counts only after
//! the human step ... mail gateways that pre-open links and chat previews consume and count nothing").
//! The human step is `count_open` (counters.rs), the verb `link_open_confirm` of the drift register
//! (X-D-03, X-D-56): it takes `ResolveCtx::human_step` and refuses without it.
//!
//! **The rules and their order.** Every rule that can refuse is one function; the order is the
//! order of this table, and the FIRST refusal answers. The audience gate comes first, so that a
//! caller who may not open the link learns nothing about its state (a forwarded `people` URL gets
//! "not for you", never "expired" or "paused"):
//!
//! | # | rule | refusal |
//! |---|---|---|
//! | 1 | the link answers to what was asked (an old id after a rotation, a dropped slug) | `NotFound` |
//! | 2 | the audience: `signed_in`, `members` (and the chosen groups), `people` (named) | `SignInRequired`, `NotAllowed` |
//! | 3 | revoked, or a `one_time` link already used | `Revoked`, `OneTimeUsed` |
//! | 4 | blocked by `ctl` (`link_blocks`, signed, 4.1) | `Blocked` |
//! | 5 | `paused` by anyone (owner, reports, operator) | `Paused` |
//! | 6 | the account's `links_disabled`: its `anyone` links are paused (3.9) | `PublicLinksOff` |
//! | 7 | `not_before` | `NotYetOpen` |
//! | 8 | `expires_at`, `expires_after_open` (from the first open), `idle_expiry` (from the last open) | `Expired` |
//! | 9 | `max_downloads`, `bytes_cap` | `DownloadsExhausted`, `BytesCapExhausted` |
//! | 10 | `max_devices`: the (n+1)th device token is refused, a known one passes | `DeviceTokenNeeded`, `DeviceLimit` |
//! | 11 | the daily download cap of a public (`anyone`) link, with the time to the next day start | `DailyCapReached` |
//! | 12 | `approval`: the caller's member must hold a wrap of the share key | `NeedsApproval` |
//! | 13 | `password`: the login of S16e must have passed | `LoginNeeded` |
//!
//! **Who calls what.**
//!
//! | unit | calls |
//! |---|---|
//! | LINKS-NODE, verb `link_resolve` | `resolve` with a `ResolveCtx` it builds from the credential, the registry and `ctl`; `MissTable` around it |
//! | LINKS-NODE, verb `link_open_confirm` (X-D-03) | `count_open` with `human_step: true` and the recipient's acknowledgement |
//! | LINKS-NODE, `object_get` of a link credential | `may_read`, `admit_bytes`, `count_bytes_served`, `count_download_complete` |
//! | LINKS-NODE, credential refresh | `renew` |
//! | LINKS-NODE, the notices | `Opened::notify` becomes `link_used` (S33), `Opened::log_entry` goes to the open log, `ResolveRefusal::ends_link` ends the paired request (S16f) |
//! | S16e (`link_login`) | `password_login_needed` before the login, `ResolveCtx::password_verified` after it |
//! | S9 (snapshots) | `LinkRecord::state_hash` |
//!
//! **The caller is decided by the node.** `Caller::Member` is a member of the link's own account (the
//! node reads the registry; the record holds no account, drift row X-DELTA-W1S16C1-20), `Caller::SignedIn`
//! any other Azlin member. A caller kind the node gets wrong can only make the crate refuse (an
//! outsider passed as `SignedIn` fails a `members` link): the crate never invents a right.
//!
//! Implements planning/architecture.md section 5.12 ("Resolving", "Scanners and previews", "Links used
//! as a CDN", "Reports"), section 3.9 (`links_disabled`), section 5.15 (M7) and section 12 (E5).

use std::collections::BTreeSet;

use azlin_seams::{AccountId, ByteCount, GroupId, MemberId, Span, Stamp};

use crate::audience::Audience;
use crate::credential::{self, CredentialSpec};
use crate::id::{PublicLinkId, Slug};
use crate::open_log;
use crate::record::{DeviceToken, LinkEnded, LinkRecord, ObjectRef, RequestId};

/// The four mutation guards the brief names for the resolver (`azlin_seams::mutant!`; COMMON-W1R
/// R19): the hold that proves the tests can fail runs
/// `AZLIN_MUTANT=<id> cargo test -p azlin-links --features azlin-seams/mutants` once for each id.
/// `RESOLVER_GUARDS` lists these four and the guards of the rest of the resolver, each with its
/// killer test.
///
/// | id | what the guard turns off | killer |
/// |---|---|---|
/// | `links_resolve_counts_open` | "an open counts only after the human step" | `open_counts_only_after_the_human_step` |
/// | `links_one_time_used_at_resolve` | `one_time` is used up when the first complete download ends | `one_time_is_used_up_when_the_first_complete_download_ends` |
/// | `links_max_devices_unbounded` | the (n+1)th device token is refused | `max_devices_refuses_the_n_plus_first_token_and_passes_known_ones` |
/// | `links_credential_ttl_unbounded` | the credential lives 15 minutes at most | `credential_is_15_minutes_and_limited_to_the_links_grants` |
///
/// Implements planning/architecture.md section 12 (E5) and section 1.4 rule 1.
///
/// Seam: S47d (its escalation tests attack the same four rules), the mutant hold of lane W1-S16c2.
pub const MUTANT_IDS: [&str; 4] = [
    "links_resolve_counts_open",
    "links_one_time_used_at_resolve",
    "links_max_devices_unbounded",
    "links_credential_ttl_unbounded",
];

/// Every mutation guard of lane S16c2's files, with the test that must fail when the guard is turned
/// off (the four of `MUTANT_IDS` among them). `tests/determinism_tests.rs` checks that these are
/// exactly the guards in the six files of the lane and that each killer exists.
///
/// Implements planning/architecture.md section 12 (E5) and section 1.4 rule 1.
///
/// Seam: the mutant hold of lane W1-S16c2, S47d.
pub const RESOLVER_GUARDS: &[(&str, &str)] = &[
    (
        "links_stale_id_resolves",
        "the_old_id_and_a_dropped_slug_resolve_nothing",
    ),
    (
        "links_audience_unchecked",
        "a_stranger_learns_nothing_about_the_links_state",
    ),
    ("links_revoked_link_resolves", "revoked"),
    ("links_blocked_link_resolves", "blocked_by_ctl"),
    ("links_paused_link_resolves", "paused"),
    (
        "links_public_links_off_ignored",
        "links_disabled_pauses_anyone_links_only_named_people_keep_working",
    ),
    ("links_not_before_ignored", "not_before_in_the_future"),
    ("links_expires_at_ignored", "expires_at_passed"),
    (
        "links_expires_after_open_ignored",
        "expires_after_open_counts_from_the_first_open",
    ),
    (
        "links_idle_expiry_ignored",
        "idle_expiry_counts_days_without_an_open",
    ),
    ("links_one_time_ignored", "one_time_used"),
    ("links_downloads_cap_ignored", "max_downloads_exhausted"),
    (
        "links_bytes_cap_ignored",
        "bytes_cap_exhausted_and_admit_bytes_truncates",
    ),
    (
        "links_max_devices_unbounded",
        "max_devices_refuses_the_n_plus_first_token_and_passes_known_ones",
    ),
    (
        "links_daily_cap_ignored",
        "daily_cap_refuses_with_retry_after_the_next_day_start",
    ),
    (
        "links_approval_unchecked",
        "approval_unapproved_needs_approval",
    ),
    (
        "links_password_unchecked",
        "password_audience_needs_login_first",
    ),
    (
        "links_resolve_counts_open",
        "open_counts_only_after_the_human_step",
    ),
    (
        "links_one_time_used_at_resolve",
        "one_time_is_used_up_when_the_first_complete_download_ends",
    ),
    (
        "links_one_time_never_used_up",
        "one_time_is_used_up_when_the_first_complete_download_ends",
    ),
    (
        "links_credential_ttl_unbounded",
        "credential_is_15_minutes_and_limited_to_the_links_grants",
    ),
    (
        "links_grants_ignored",
        "a_credential_reads_only_what_the_grants_name",
    ),
    (
        "links_stale_credential_renewed",
        "a_credential_of_another_key_or_an_old_one_is_not_renewed",
    ),
    (
        "links_renewal_skips_rules",
        "renewal_rechecks_every_option_and_counts_nothing",
    ),
    ("links_scopes_widen_on_renewal", "scopes_only_narrow"),
    (
        "links_open_log_unacknowledged",
        "open_on_a_logged_link_needs_the_acknowledgement",
    ),
    (
        "links_notify_first_open_every_time",
        "notify_never_first_every",
    ),
    (
        "links_miss_budget_unbounded",
        "miss_budget_refuses_after_capacity_with_retry_after",
    ),
    (
        "links_miss_table_unbounded",
        "table_is_bounded_and_evicts_the_oldest",
    ),
    (
        "links_admit_bytes_unbounded",
        "bytes_cap_exhausted_and_admit_bytes_truncates",
    ),
    (
        "links_download_beyond_cap_counted",
        "max_downloads_n_admits_exactly_n_completions_over_5000_seeded_schedules",
    ),
    (
        "links_ended_download_counted",
        "a_completion_after_the_link_ended_counts_no_download",
    ),
];

/// The seconds of a day: day starts are UTC midnights (D37; planning/architecture.md section 5.3).
const SECS_PER_DAY: u64 = 86_400;

/// The operator's blocks of single links: `link_blocks` of `ctl` ("signed, 4.1"; "block the link
/// (`link_blocks`, every node at once)", section 3.9). The node verifies the operator's signatures
/// and keeps the set; this trait only asks.
///
/// Implements planning/architecture.md section 5.12 ("`link_blocks` in `ctl` (signed, 4.1) stay for
/// takedowns") and section 3.9.
///
/// Caller: `resolve`, `renew`, `count_open`, `admit_bytes` (through the context); Seam: LINKS-NODE
/// (it implements the trait over the signed rows of `ctl`), AzCtl's takedown rows (S45).
pub trait LinkBlocks {
    /// Whether the operator has blocked the link `link`.
    fn is_blocked(&self, link: &PublicLinkId) -> bool;
}

/// A plain set of blocked link ids is a block list.
impl LinkBlocks for BTreeSet<PublicLinkId> {
    fn is_blocked(&self, link: &PublicLinkId) -> bool {
        self.contains(link)
    }
}

/// Who asks, as the node knows it from the credential on the connection and from the registry.
///
/// Implements planning/architecture.md section 5.12 (`audience`: `anyone`, `signed_in`, `members`,
/// `people`, `approval`) and section 5.11 (a credential names `account`, `member`, `device`).
///
/// Caller: `ResolveCtx`, `open_log::entry_for`; Seam: LINKS-NODE (it builds the caller; `Member` only
/// for a member of the link's own account).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Caller {
    /// Nobody is signed in: an anonymous link recipient (a browser, a scanner).
    Anonymous,
    /// A member of any Azlin account (not known to be a member of the link's own account).
    SignedIn {
        /// The member's account.
        account: AccountId,
        /// The member.
        member: MemberId,
    },
    /// A member of the link's own account, with the groups of that account it belongs to.
    Member {
        /// The member's account (the link's own).
        account: AccountId,
        /// The member.
        member: MemberId,
        /// The groups of the account the member belongs to.
        groups: BTreeSet<GroupId>,
    },
}

impl Caller {
    /// The member, for a signed-in caller.
    pub fn member(&self) -> Option<&MemberId> {
        match self {
            Caller::Anonymous => None,
            Caller::SignedIn { member, .. } | Caller::Member { member, .. } => Some(member),
        }
    }

    /// The account of the member, for a signed-in caller.
    pub fn account(&self) -> Option<&AccountId> {
        match self {
            Caller::Anonymous => None,
            Caller::SignedIn { account, .. } | Caller::Member { account, .. } => Some(account),
        }
    }

    /// Whether the caller is signed in.
    pub fn is_signed_in(&self) -> bool {
        !matches!(self, Caller::Anonymous)
    }
}

/// What the caller asked for in the URL `https://drive.azlin.io/l/<link id or slug>`.
///
/// Implements planning/architecture.md section 5.12 ("The URL": "`/l/<link id or slug>`"; "after
/// which the old URL stops at once").
///
/// Caller: `ResolveCtx`; Seam: LINKS-NODE (it parses the URL: S16d's `parse_link` tells an id from a
/// slug).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Asked {
    /// The link id.
    Id(PublicLinkId),
    /// The readable name.
    Slug(Slug),
}

/// Everything about the request that the rules need and the record does not hold: what was asked,
/// when, by whom, from which device, the operator's blocks, the account's `links_disabled`, the
/// day's bytes and cap, and what the node knows of the password login and of the human step. Every
/// field is a fact the node states: there is no default.
///
/// Implements planning/architecture.md section 5.12 ("Resolving", "Links used as a CDN", "Scanners
/// and previews") and section 3.9 (`links_disabled`).
///
/// Caller: `resolve`, `renew`, `count_open`, `admit_bytes`; Seam: LINKS-NODE (it builds one for each
/// request).
#[derive(Clone)]
pub struct ResolveCtx<'a> {
    /// What the URL named: the id the link must still answer to, or its slug.
    pub asked: Asked,
    /// The leader's stamp of the request (section 2.4: time is the leader's stamp, checked).
    pub now: Stamp,
    /// Who asks.
    pub caller: Caller,
    /// The random token the client keeps (`max_devices` counts these, never an address).
    pub device: Option<DeviceToken>,
    /// The operator's blocks (`link_blocks` of `ctl`).
    pub blocks: &'a dyn LinkBlocks,
    /// The account has lost its public links (section 3.9: `links_disabled`).
    pub links_disabled: bool,
    /// The bytes the link has served today (the node counts them as it serves, in RAM).
    pub served_today: ByteCount,
    /// The daily download cap of a public link, set per plan (section 5.12, "Links used as a CDN").
    pub daily_cap: ByteCount,
    /// This caller has passed `link_login` for this link (S16e), or holds a link credential that was
    /// issued after it.
    pub password_verified: bool,
    /// The request is the human step: the command `link_open_confirm` (the "Open" or "Download"
    /// button), never a page load or a preview. Only `count_open` reads it.
    pub human_step: bool,
}

/// Why a link answers `Expired`: which of the three expiries ended it.
///
/// Implements planning/architecture.md section 5.12 (`expires_at`, `expires_after_open`,
/// `idle_expiry`).
///
/// Caller: `ResolveRefusal::Expired`, `ResolveRefusal::ends_link`; Seam: the apps (they say which).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum ExpiredBy {
    /// `expires_at`: the date came.
    Date,
    /// `expires_after_open`: the time from the first open has passed.
    AfterOpen,
    /// `idle_expiry`: the link was not opened for that long.
    Idle,
}

words!(
    ExpiredBy,
    "link expiry",
    [Date => "expires_at", AfterOpen => "expires_after_open", Idle => "idle_expiry"]
);

/// Why a link does not open for a caller now. One variant for each rule of the table of this module;
/// the node maps each to a status and carries `reason()` as the word. The record is not changed.
///
/// Implements planning/architecture.md section 5.12 ("Resolving": "checks every server option
/// (dates, counts, devices, `paused`, blocks, the audience)"), section 3.9 and section 2.5 (status
/// codes).
///
/// Caller: `resolve`, `renew`, `count_open` (`OpenRefusal::Link`), `count_download_complete`;
/// Seam: LINKS-NODE (it answers the status and the reason word).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum ResolveRefusal {
    /// The link does not answer to what was asked: an id it had before a rotation, or a dropped slug.
    #[error("no such link")]
    NotFound,
    /// The audience needs a signed-in caller.
    #[error("sign in to open this link")]
    SignInRequired,
    /// The caller is signed in and is not in the audience.
    #[error("this link is not for you")]
    NotAllowed,
    /// `approval`: the caller's member holds no wrap of the share key yet; the ask goes through the
    /// link's paired request.
    #[error("this link opens for people its sender approves")]
    NeedsApproval {
        /// The paired request that carries the ask.
        request: Option<RequestId>,
    },
    /// `password`: the login (`link_login`) has not passed.
    #[error("this link needs its password: log in first")]
    LoginNeeded,
    /// The link is revoked.
    #[error("the link is revoked")]
    Revoked,
    /// A `one_time` link whose first complete download has happened.
    #[error("this one-time link has been used")]
    OneTimeUsed,
    /// The operator has blocked the link (`link_blocks`).
    #[error("the link is blocked")]
    Blocked,
    /// The link is paused.
    #[error("the link is paused")]
    Paused,
    /// The account has lost its public links (`links_disabled`) and this is one.
    #[error("public links of this account are paused")]
    PublicLinksOff,
    /// `not_before`: the link opens later.
    #[error("the link opens later")]
    NotYetOpen {
        /// When it opens.
        opens_at: Stamp,
    },
    /// One of the three expiries has ended the link.
    #[error("the link has expired ({by})")]
    Expired {
        /// Which expiry.
        by: ExpiredBy,
    },
    /// `max_downloads` complete downloads have happened.
    #[error("the link's downloads are used up")]
    DownloadsExhausted,
    /// `bytes_cap` bytes have been served.
    #[error("the link's bytes are used up")]
    BytesCapExhausted,
    /// `max_devices` is set and the request carried no device token.
    #[error("this link counts devices: a device token is needed")]
    DeviceTokenNeeded,
    /// `max_devices` devices have opened the link and this one is a new one.
    #[error("the link has reached its devices")]
    DeviceLimit,
    /// A public link has served its daily cap: come back at the next day start.
    #[error("today's download cap of this link is reached")]
    DailyCapReached {
        /// The time to the next day start (at least one second).
        retry_after: Span,
    },
    /// `renew`: the credential is of another key (the link was rotated), has ended, or does not read
    /// this link: resolve the link again.
    #[error("the credential is stale: resolve the link again")]
    StaleCredential,
}

impl ResolveRefusal {
    /// The word of the refusal, one for each variant: what the node puts into the answer's `reason`.
    ///
    /// Implements planning/architecture.md section 2.5 (status codes carry a reason).
    pub const fn reason(&self) -> &'static str {
        match self {
            ResolveRefusal::NotFound => "not_found",
            ResolveRefusal::SignInRequired => "sign_in_required",
            ResolveRefusal::NotAllowed => "not_allowed",
            ResolveRefusal::NeedsApproval { .. } => "needs_approval",
            ResolveRefusal::LoginNeeded => "login_needed",
            ResolveRefusal::Revoked => "revoked",
            ResolveRefusal::OneTimeUsed => "one_time_used",
            ResolveRefusal::Blocked => "blocked",
            ResolveRefusal::Paused => "paused",
            ResolveRefusal::PublicLinksOff => "public_links_off",
            ResolveRefusal::NotYetOpen { .. } => "not_yet_open",
            ResolveRefusal::Expired { .. } => "expired",
            ResolveRefusal::DownloadsExhausted => "downloads_exhausted",
            ResolveRefusal::BytesCapExhausted => "bytes_cap_exhausted",
            ResolveRefusal::DeviceTokenNeeded => "device_token_needed",
            ResolveRefusal::DeviceLimit => "device_limit",
            ResolveRefusal::DailyCapReached { .. } => "daily_cap_reached",
            ResolveRefusal::StaleCredential => "stale_credential",
        }
    }

    /// The delay the node answers with (`retry_after`, class After): only the daily cap has one.
    pub const fn retry_after(&self) -> Option<Span> {
        match self {
            ResolveRefusal::DailyCapReached { retry_after } => Some(*retry_after),
            _ => None,
        }
    }

    /// How the link has ended, when this refusal says it has: the paired request ends with it (S16f's
    /// `end_with_link`). A revoked, used one-time or blocked link ended as `Revoked`, an expired one
    /// as `Expired`; a pause, a cap, a caller or a clock does not end a link.
    ///
    /// Implements planning/architecture.md section 5.12 ("a paired request ends with its link").
    pub const fn ends_link(&self) -> Option<LinkEnded> {
        match self {
            ResolveRefusal::Revoked | ResolveRefusal::OneTimeUsed | ResolveRefusal::Blocked => {
                Some(LinkEnded::Revoked)
            }
            ResolveRefusal::Expired { .. } => Some(LinkEnded::Expired),
            ResolveRefusal::NotFound
            | ResolveRefusal::SignInRequired
            | ResolveRefusal::NotAllowed
            | ResolveRefusal::NeedsApproval { .. }
            | ResolveRefusal::LoginNeeded
            | ResolveRefusal::Paused
            | ResolveRefusal::PublicLinksOff
            | ResolveRefusal::NotYetOpen { .. }
            | ResolveRefusal::DownloadsExhausted
            | ResolveRefusal::BytesCapExhausted
            | ResolveRefusal::DeviceTokenNeeded
            | ResolveRefusal::DeviceLimit
            | ResolveRefusal::DailyCapReached { .. }
            | ResolveRefusal::StaleCredential => None,
        }
    }
}

/// What `link_resolve` gives back: where the manifest is, the credential to read it with, and whether
/// the recipient must be told about the open log before opening.
///
/// Implements planning/architecture.md section 5.12 ("Resolving": "returns the manifest object and a
/// **link credential**"; `open_log`: "the recipient is told before opening (M7)").
///
/// Caller: `resolve`; Seam: LINKS-NODE (it signs `credential`, answers `manifest_object`, and tells
/// the app to show the open-log notice).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Resolved {
    /// The AZS2 manifest object of the link.
    pub manifest_object: ObjectRef,
    /// The credential to issue: 15 minutes at most, limited to the link.
    pub credential: CredentialSpec,
    /// The link keeps an open log: the app shows the notice, and `count_open` wants the
    /// acknowledgement.
    pub tell_recipient_open_log: bool,
}

/// What a link that passes every rule tells the caller of `check_open`: the earliest date by which a
/// time rule ends it (the credential does not outlive it).
pub(crate) struct Opening {
    /// The earliest end by `expires_at`, `expires_after_open` or `idle_expiry`; none when no date
    /// bounds the link.
    pub(crate) deadline: Option<Stamp>,
}

/// Whether the link is a public one: `anyone` (section 3.9: "public (`anyone`) links"). The daily
/// cap and `links_disabled` are about these.
pub(crate) fn is_public(r: &LinkRecord) -> bool {
    matches!(r.options().audience, Audience::Anyone)
}

/// The time to the next day start (UTC midnight) from `now`: between one second and one day.
pub(crate) fn until_next_day(now: Stamp) -> Span {
    Span::secs(SECS_PER_DAY - now.as_secs() % SECS_PER_DAY)
}

/// Rule 1: the link answers to what was asked.
fn gate_asked(r: &LinkRecord, asked: &Asked) -> Result<(), ResolveRefusal> {
    let answers = match asked {
        Asked::Id(id) => r.answers_to(id),
        Asked::Slug(slug) => r.answers_to_slug(slug),
    };
    if answers || azlin_seams::mutant!("links_stale_id_resolves") {
        Ok(())
    } else {
        Err(ResolveRefusal::NotFound)
    }
}

/// Rule 2: the audience. `approval` lets everyone through here ("anyone may ask"): its gate is rule
/// 12, after the rules about the link's state.
fn gate_audience(r: &LinkRecord, caller: &Caller) -> Result<(), ResolveRefusal> {
    if azlin_seams::mutant!("links_audience_unchecked") {
        return Ok(());
    }
    match &r.options().audience {
        Audience::Anyone | Audience::Approval => Ok(()),
        Audience::SignedIn => {
            if caller.is_signed_in() {
                Ok(())
            } else {
                Err(ResolveRefusal::SignInRequired)
            }
        }
        Audience::Members { groups } => match caller {
            Caller::Anonymous => Err(ResolveRefusal::SignInRequired),
            Caller::SignedIn { .. } => Err(ResolveRefusal::NotAllowed),
            Caller::Member { groups: mine, .. } => {
                if groups.is_empty() || groups.iter().any(|group| mine.contains(group)) {
                    Ok(())
                } else {
                    Err(ResolveRefusal::NotAllowed)
                }
            }
        },
        Audience::People { people } => match caller.member() {
            None => Err(ResolveRefusal::SignInRequired),
            Some(member) => {
                if people.contains(member) {
                    Ok(())
                } else {
                    Err(ResolveRefusal::NotAllowed)
                }
            }
        },
    }
}

/// Rule 3: a revoked link and a used `one_time` link are over for good.
pub(crate) fn gate_ended(r: &LinkRecord) -> Result<(), ResolveRefusal> {
    if r.options().one_time
        && r.counters().downloads >= 1
        && !azlin_seams::mutant!("links_one_time_ignored")
    {
        return Err(ResolveRefusal::OneTimeUsed);
    }
    if r.is_revoked() && !azlin_seams::mutant!("links_revoked_link_resolves") {
        return Err(ResolveRefusal::Revoked);
    }
    Ok(())
}

/// Rules 4 to 6: the operator's block, any pause, and the account's `links_disabled` for a public
/// link.
fn gate_blocked_and_paused(r: &LinkRecord, c: &ResolveCtx<'_>) -> Result<(), ResolveRefusal> {
    if c.blocks.is_blocked(r.id()) && !azlin_seams::mutant!("links_blocked_link_resolves") {
        return Err(ResolveRefusal::Blocked);
    }
    if r.is_paused() && !azlin_seams::mutant!("links_paused_link_resolves") {
        return Err(ResolveRefusal::Paused);
    }
    if c.links_disabled && is_public(r) && !azlin_seams::mutant!("links_public_links_off_ignored") {
        return Err(ResolveRefusal::PublicLinksOff);
    }
    Ok(())
}

/// The earlier of the current deadline and `other`.
fn earliest(current: Option<Stamp>, other: Stamp) -> Stamp {
    match current {
        Some(at) if at <= other => at,
        _ => other,
    }
}

/// Rules 7 and 8: the dates. `not_before` and `expires_at` are dates; `expires_after_open` counts
/// from the first open; `idle_expiry` from the last open, or, for a link never opened, from the time
/// its current key began (`keyed_at`, never `created_at`: a rotation starts the idle time again). A
/// link ends AT its end (`now >= end`). Returns the earliest end that lies ahead. An end beyond the
/// largest stamp is never reached.
fn gate_dates(r: &LinkRecord, now: Stamp) -> Result<Option<Stamp>, ResolveRefusal> {
    let options = r.options();
    let counters = r.counters();
    if let Some(opens_at) = options.not_before {
        if now < opens_at && !azlin_seams::mutant!("links_not_before_ignored") {
            return Err(ResolveRefusal::NotYetOpen { opens_at });
        }
    }
    let mut deadline: Option<Stamp> = None;
    if let Some(end) = options.expires_at {
        if now >= end && !azlin_seams::mutant!("links_expires_at_ignored") {
            return Err(ResolveRefusal::Expired {
                by: ExpiredBy::Date,
            });
        }
        deadline = Some(earliest(deadline, end));
    }
    if let (Some(span), Some(opened)) = (options.expires_after_open, counters.first_open_at) {
        if let Some(end) = opened.checked_add(span) {
            if now >= end && !azlin_seams::mutant!("links_expires_after_open_ignored") {
                return Err(ResolveRefusal::Expired {
                    by: ExpiredBy::AfterOpen,
                });
            }
            deadline = Some(earliest(deadline, end));
        }
    }
    if let Some(span) = options.idle_expiry {
        let last = match (counters.last_open_at, counters.first_open_at) {
            (Some(at), _) | (None, Some(at)) => at,
            (None, None) => r.keyed_at(),
        };
        if let Some(end) = last.checked_add(span) {
            if now >= end && !azlin_seams::mutant!("links_idle_expiry_ignored") {
                return Err(ResolveRefusal::Expired {
                    by: ExpiredBy::Idle,
                });
            }
            deadline = Some(earliest(deadline, end));
        }
    }
    Ok(deadline)
}

/// Rule 9, the downloads: `max_downloads` complete downloads have happened.
pub(crate) fn gate_downloads(r: &LinkRecord) -> Result<(), ResolveRefusal> {
    if let Some(max) = r.options().max_downloads {
        if r.counters().downloads >= u64::from(max)
            && !azlin_seams::mutant!("links_downloads_cap_ignored")
        {
            return Err(ResolveRefusal::DownloadsExhausted);
        }
    }
    Ok(())
}

/// Rule 9, the bytes: `bytes_cap` bytes have been served.
fn gate_bytes(r: &LinkRecord) -> Result<(), ResolveRefusal> {
    if let Some(cap) = r.options().bytes_cap {
        if r.counters().bytes_served >= cap && !azlin_seams::mutant!("links_bytes_cap_ignored") {
            return Err(ResolveRefusal::BytesCapExhausted);
        }
    }
    Ok(())
}

/// Rule 10: `max_devices`. A link that counts devices needs the client's token; a known token
/// passes, a new one only while the link has fewer than `max_devices` devices.
fn gate_devices(r: &LinkRecord, device: Option<&DeviceToken>) -> Result<(), ResolveRefusal> {
    let Some(max) = r.options().max_devices else {
        return Ok(());
    };
    let Some(token) = device else {
        return Err(ResolveRefusal::DeviceTokenNeeded);
    };
    let seen = &r.counters().devices_seen;
    let known = seen.contains(&token.seen_by(r.id()));
    if !known
        && seen.len() >= usize::from(max)
        && !azlin_seams::mutant!("links_max_devices_unbounded")
    {
        return Err(ResolveRefusal::DeviceLimit);
    }
    Ok(())
}

/// Rule 11: the daily download cap of a public link, with the time to the next day start.
fn gate_daily_cap(r: &LinkRecord, c: &ResolveCtx<'_>) -> Result<(), ResolveRefusal> {
    if is_public(r)
        && c.served_today >= c.daily_cap
        && !azlin_seams::mutant!("links_daily_cap_ignored")
    {
        return Err(ResolveRefusal::DailyCapReached {
            retry_after: until_next_day(c.now),
        });
    }
    Ok(())
}

/// Whether the caller is approved for an `approval` link: its member holds a wrap of the share key.
fn is_approved(r: &LinkRecord, caller: &Caller) -> bool {
    caller
        .member()
        .is_some_and(|member| r.wrap_for(member).is_some())
}

/// Rule 12: an `approval` link opens for the members the sender has approved.
fn gate_approval(r: &LinkRecord, caller: &Caller) -> Result<(), ResolveRefusal> {
    if matches!(r.options().audience, Audience::Approval)
        && !is_approved(r, caller)
        && !azlin_seams::mutant!("links_approval_unchecked")
    {
        return Err(ResolveRefusal::NeedsApproval {
            request: r.request_id().copied(),
        });
    }
    Ok(())
}

/// Rule 13: a `password` link opens after the login has passed.
fn gate_password(r: &LinkRecord, proven: bool) -> Result<(), ResolveRefusal> {
    if r.options().password && !proven && !azlin_seams::mutant!("links_password_unchecked") {
        return Err(ResolveRefusal::LoginNeeded);
    }
    Ok(())
}

/// Every rule, in the order of the table of this module: the one question that `resolve`, `renew`,
/// `count_open` and `admit_bytes` all ask. `password_proven` is whether the password gate is
/// satisfied (a renewal's old credential proves the login). It reads the record and the context and
/// changes nothing.
pub(crate) fn check_open(
    r: &LinkRecord,
    c: &ResolveCtx<'_>,
    password_proven: bool,
) -> Result<Opening, ResolveRefusal> {
    gate_asked(r, &c.asked)?;
    gate_audience(r, &c.caller)?;
    gate_ended(r)?;
    gate_blocked_and_paused(r, c)?;
    let deadline = gate_dates(r, c.now)?;
    gate_downloads(r)?;
    gate_bytes(r)?;
    gate_devices(r, c.device.as_ref())?;
    gate_daily_cap(r, c)?;
    gate_approval(r, &c.caller)?;
    gate_password(r, password_proven)?;
    Ok(Opening { deadline })
}

/// Resolves a link: checks every server option and the audience against the caller, and returns the
/// manifest object and the credential to issue. It COUNTS NOTHING (the record is borrowed shared):
/// no counter, no `first_open_at`, no device, no notice, no log entry; a scanner that resolves a
/// link a thousand times changes no state ("an open counts only after the human step", section 5.12).
/// A link with a password needs the login first (`LoginNeeded`, then `password_verified`), an
/// `approval` link needs the approval (`NeedsApproval`).
///
/// Implements planning/architecture.md section 5.12 ("Resolving", "Scanners and previews"), section
/// 3.9 and section 12 (E5: `link_resolve` is a command before sign-in).
///
/// Seam: LINKS-NODE (the verb `link_resolve`; the budget of misses is `MissTable`'s).
pub fn resolve(r: &LinkRecord, c: &ResolveCtx<'_>) -> Result<Resolved, ResolveRefusal> {
    let opening = check_open(r, c, c.password_verified)?;
    Ok(Resolved {
        manifest_object: *r.manifest_object(),
        credential: credential::issue(r, &opening, c.now),
        tell_recipient_open_log: open_log::is_logged(r),
    })
}

/// Whether the link has a password, so that `link_login` (S16e) runs before the link opens: the
/// question S16e's routing asks BEFORE `resolve` (`resolve` itself refuses a password link until the
/// login has passed).
///
/// Implements planning/architecture.md section 5.12 ("Passwords with OPAQUE": "opening runs
/// `link_login`").
///
/// Seam: LINKS-NODE (it routes the client to `link_login`), S16e.
pub fn password_login_needed(r: &LinkRecord) -> bool {
    r.options().password
}
