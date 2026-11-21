//! Changing a link (planning/architecture.md section 5.12 "Changing a link"):
//!
//! - "server options change at any time (`link_update`)": [`update`] with a [`Change`] of a server
//!   option, of a wrap added (a person added, an approval given), of a pairing;
//! - "who can decrypt changes only by **rotating**: a new id and key, after which the old URL stops
//!   at once": a [`Change`] that would change who can decrypt (the audience across key strength or
//!   between the fragment and the wraps, a wrap removed, the password, the kind, the subfolders, the
//!   grants) is refused with `RotationRequired`, and [`rotate`] is the one function that makes a
//!   new id and takes the key-bound parts in ([`KeyBound`]): the manifest sealed under the new key,
//!   the new wraps, the new OPAQUE record, the new paired request;
//! - "`paused` stops and resumes a link without changing it, and `link_revoke` ends it": [`pause`],
//!   [`unpause`] and [`revoke`] (revoking is final);
//! - "Live folders (`folder_live`): writers re-point the link at each new version": [`repoint`].
//!
//! **Who may**: only the creator or a manager of the link's space changes a link; a writer re-points a
//! live folder; the reports and the operator pause, and the operator lifts the pauses of the
//! reports and its own and may revoke (a takedown). [`Actor`] carries what the node learned from the
//! credential and the space's grants (`write` and `manage` come from `azlin_spaces::access`, mapped
//! by the glue, COMMON-W1R R14); this crate holds no account or space state.
//!
//! **Every change is judged whole**: the function builds the new row, checks every invariant of a
//! row ([`RecordError`]) and, for the options the change touches, the account's `link_policy`
//! ([`PolicyViolation`]: a forced option may not be undone, an expiry may not pass the policy's
//! longest life), and only then replaces the record. A refused change leaves the record exactly as it
//! was (a property test applies thousands of random changes and checks it).
//!
//! **A frozen link**: while the operator or the reports hold a pause, the owner changes nothing
//! (`update`, `rotate` and `repoint` answer `Frozen`): the operator decides about the link as it
//! is. The owner can still revoke. A pause of the owner's own freezes nothing (drift register of
//! lane W1-S16c1).
//!
//! **Rotation resets what the old URL's holders did**: the counters start again (devices are hashed
//! with the link's id, so the old hashes mean nothing under the new one), `keyed_at` is the time of
//! the rotation, `created_at` stays (the policy's longest life counts from it), the paired request
//! ends (`Rotated::ended_request`: S16f's `end_with_link(Rotated)`) and the new one comes with the
//! rotation, the comment log and the pauses stay.
//!
//! Implements planning/architecture.md section 5.12 ("Changing a link", "Live folders").

use std::collections::BTreeMap;

use azlin_policy::LinkPolicy;
use azlin_seams::{ByteCount, Entropy, EntropyError, MemberId, Span, Stamp};

use crate::audience::Audience;
use crate::bounds::{violations, PolicyViolation};
use crate::id::{new_id, PublicLinkId, Slug};
use crate::kind::LinkKind;
use crate::options::{Notify, Rights, Sealed};
use crate::record::{
    check_row, Counters, Grants, LinkRecord, LinkRow, LogId, ObjectRef, OpaqueRecord, PausedBy,
    Pauses, RecordError, RequestId, Wrap,
};
use crate::strength::OptionName;
use crate::validate::OptionError;

/// Who acts on a link, as the node knows it from the credential and the space's grants.
///
/// Implements planning/architecture.md section 5.12 ("Changing a link"; "a link reported from three
/// different devices pauses until decided") and section 3.9.
///
/// Caller: `update`, `rotate`, `pause`, `unpause`, `revoke`, `repoint`; Seam: LINKS-NODE (it builds
/// `Member` from the credential and `azlin_spaces::access` of the link's space), S46 (`Reports`),
/// AzCtl's operator rows (`Operator`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Actor {
    /// A member acting through a device.
    Member {
        /// The member.
        member: MemberId,
        /// The member may write to the link's space (an editor or a manager): needed to re-point a
        /// live folder.
        write: bool,
        /// The member manages the link's space: may change any link of it.
        manage: bool,
    },
    /// The reports counting of 3.9: it pauses a link and does nothing else.
    Reports,
    /// The operator: pauses, lifts the pauses of the reports and its own, and revokes.
    Operator,
}

/// Why a change needs a rotation: what it would change about who can decrypt.
///
/// Implements planning/architecture.md section 5.12 ("who can decrypt changes only by rotating").
///
/// Caller: `UpdateRefusal::RotationRequired`; Seam: the share dialog (it explains the new URL).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum RotationReason {
    /// The audience changes across key strength, or between the fragment and the wraps.
    Audience,
    /// A wrap is removed: its holder knows the key.
    WrapRemoved,
    /// The password changes: the link's real key derives from the password's export key.
    Password,
    /// The kind changes: the manifest holds keys only for what is shared.
    Kind,
    /// The subfolders change: the same.
    Subfolders,
    /// The grants change.
    Grants,
}

/// One change of a link: the server options (any time), the pairings and a wrap added (no
/// rotation), and the changes of who can decrypt (a rotation).
///
/// Implements planning/architecture.md section 5.12 ("Changing a link").
///
/// Caller: `update`; Seam: LINKS-NODE (`link_update` decodes one), the share dialog.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Change {
    /// `app_only`.
    AppOnly(bool),
    /// `not_before`.
    NotBefore(Option<Stamp>),
    /// `expires_at`.
    ExpiresAt(Option<Stamp>),
    /// `expires_after_open`.
    ExpiresAfterOpen(Option<Span>),
    /// `idle_expiry`.
    IdleExpiry(Option<Span>),
    /// `one_time`.
    OneTime(bool),
    /// `max_downloads`.
    MaxDownloads(Option<u32>),
    /// `bytes_cap`.
    BytesCap(Option<ByteCount>),
    /// `max_devices`.
    MaxDevices(Option<u16>),
    /// `download` or `preview_only`.
    Rights(Rights),
    /// `allow_upload` (the link needs its paired request).
    AllowUpload(bool),
    /// `edit` (a named audience only).
    Edit(bool),
    /// `comment` (a signed-in audience, and the comment log).
    Comment(bool),
    /// `allow_reshare`.
    AllowReshare(bool),
    /// `watermark`.
    Watermark(bool),
    /// `notify`.
    Notify(Notify),
    /// `open_log` (a signed-in audience only).
    OpenLog(bool),
    /// `title`, sealed under the same link key.
    Title(Option<Sealed>),
    /// `message`, sealed under the same link key.
    Message(Option<Sealed>),
    /// `slug` (a password or a named audience only). Slugs are unique per cluster: the node checks
    /// its index of slugs before it calls `update`, this crate holds no other link's slug.
    Slug(Option<Slug>),
    /// The paired request of the link, when it has none yet.
    PairRequest(RequestId),
    /// The comment log of the link, when it has none yet.
    PairCommentLog(LogId),
    /// A wrap of the share key: a person added to a `people` link, or an approval given on an
    /// `approval` link ("approving wraps the share key to that key (`link_update`)").
    AddWrap(Wrap),
    /// The audience: without a rotation only between `signed_in` and `members` and from one list
    /// of groups to another.
    Audience(Audience),
    /// A wrap removed (a person removed, an approval taken back): needs a rotation.
    RemoveWrap(MemberId),
    /// The password (and its OPAQUE record): needs a rotation.
    Password(Option<OpaqueRecord>),
    /// The kind: needs a rotation.
    Kind(LinkKind),
    /// The subfolders: needs a rotation.
    Subfolders(bool),
    /// The grants: needs a rotation (a live folder's writers re-point instead: [`repoint`]).
    Grants(Grants),
}

impl Change {
    /// The options the change touches: the policy is judged for these only (a link made under an
    /// older policy can still change something else).
    ///
    /// Implements planning/architecture.md section 5.10 (`link_policy`) and section 5.12.
    pub fn touches(&self) -> &'static [OptionName] {
        match self {
            Change::AppOnly(_) => &[OptionName::AppOnly],
            Change::NotBefore(_) => &[OptionName::NotBefore],
            Change::ExpiresAt(_) => &[OptionName::ExpiresAt],
            Change::ExpiresAfterOpen(_) => &[OptionName::ExpiresAfterOpen],
            Change::IdleExpiry(_) => &[OptionName::IdleExpiry],
            Change::OneTime(_) => &[OptionName::OneTime],
            Change::MaxDownloads(_) => &[OptionName::MaxDownloads],
            Change::BytesCap(_) => &[OptionName::BytesCap],
            Change::MaxDevices(_) => &[OptionName::MaxDevices],
            Change::Rights(_) => &[OptionName::Rights],
            Change::AllowUpload(_) => &[OptionName::AllowUpload],
            Change::Edit(_) => &[OptionName::Edit],
            Change::Comment(_) => &[OptionName::Comment],
            Change::AllowReshare(_) => &[OptionName::AllowReshare],
            Change::Watermark(_) => &[OptionName::Watermark],
            Change::Notify(_) => &[OptionName::Notify],
            Change::OpenLog(_) => &[OptionName::OpenLog],
            Change::Title(_) => &[OptionName::Title],
            Change::Message(_) => &[OptionName::Message],
            Change::Slug(_) => &[OptionName::Slug],
            Change::PairRequest(_) | Change::PairCommentLog(_) | Change::Grants(_) => &[],
            Change::AddWrap(_) | Change::Audience(_) | Change::RemoveWrap(_) => {
                &[OptionName::Audience]
            }
            Change::Password(_) => &[OptionName::Password],
            Change::Kind(_) => &[OptionName::Kind],
            Change::Subfolders(_) => &[OptionName::Subfolders],
        }
    }
}

/// Why a link is not changed. The record is as it was.
///
/// Implements planning/architecture.md section 5.12 ("Changing a link").
///
/// Caller: `update`, `rotate`, `pause`, `unpause`, `revoke`, `repoint`; Seam: LINKS-NODE (it maps
/// each to a status: `forbidden`, `conflict`, `bad_request`).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum UpdateRefusal {
    /// The actor is not the creator or a manager of the link's space (or, for `repoint`, a writer).
    #[error("only the creator or a manager of the space changes a link")]
    NotPermitted,
    /// The link is revoked: a revoked link never changes.
    #[error("the link is revoked: a revoked link never changes")]
    Revoked,
    /// The operator or the reports have paused the link: nothing changes until it is lifted.
    #[error("the link is paused by {0}: nothing changes until the pause is lifted")]
    Frozen(PausedBy),
    /// The change would change who can decrypt: rotate the link.
    #[error("this changes who can decrypt: rotate the link ({0:?})")]
    RotationRequired(RotationReason),
    /// The result would break a rule between options or an invariant of the row.
    #[error("the change breaks a rule of the link: {0}")]
    Invalid(RecordError),
    /// The account's `link_policy` refuses the change.
    #[error("the account's link policy refuses the change")]
    Policy(Vec<PolicyViolation>),
    /// Only a live folder link is re-pointed.
    #[error("only a live folder link is re-pointed")]
    NotLive,
    /// The member holds no wrap of the share key.
    #[error("the member holds no wrap of the share key")]
    NoSuchWrap,
    /// The link has a paired request or a comment log already, or a rotation gives back the request
    /// that ends with the old id (a rotation brings a new one).
    #[error("the link has this pairing already")]
    AlreadyPaired,
    /// The member holds a wrap of the share key already.
    #[error("the member holds a wrap of the share key already")]
    AlreadyWrapped,
    /// The random source failed: no new id.
    #[error("the random source failed: {0}")]
    Entropy(EntropyError),
    /// The new id is the old id (a broken random source): the old URL would not stop.
    #[error("the new id is the old id")]
    SameId,
}

/// Whether `by` may change the link: the creator, or a manager of its space.
fn may_change(r: &LinkRecord, by: &Actor) -> bool {
    match by {
        Actor::Member { member, manage, .. } => {
            if azlin_seams::mutant!("links_update_by_anyone") {
                return true;
            }
            member == r.created_by() || *manage
        }
        Actor::Reports | Actor::Operator => false,
    }
}

/// A revoked link never changes.
fn guard_open(r: &LinkRecord) -> Result<(), UpdateRefusal> {
    if r.is_revoked() && !azlin_seams::mutant!("links_revoked_link_changes") {
        return Err(UpdateRefusal::Revoked);
    }
    Ok(())
}

/// The operator's and the reports' pause freezes the link for its owner.
fn guard_unfrozen(r: &LinkRecord) -> Result<(), UpdateRefusal> {
    match r.pauses().frozen_by() {
        Some(by) if !azlin_seams::mutant!("links_frozen_link_changes") => {
            Err(UpdateRefusal::Frozen(by))
        }
        _ => Ok(()),
    }
}

/// Puts `change` on the row, or says it needs a rotation.
fn apply(next: &mut LinkRow, change: Change) -> Result<(), UpdateRefusal> {
    match change {
        Change::AppOnly(on) => next.options.app_only = on,
        Change::NotBefore(at) => next.options.not_before = at,
        Change::ExpiresAt(at) => next.options.expires_at = at,
        Change::ExpiresAfterOpen(span) => next.options.expires_after_open = span,
        Change::IdleExpiry(span) => next.options.idle_expiry = span,
        Change::OneTime(on) => next.options.one_time = on,
        Change::MaxDownloads(count) => next.options.max_downloads = count,
        Change::BytesCap(cap) => next.options.bytes_cap = cap,
        Change::MaxDevices(count) => next.options.max_devices = count,
        Change::Rights(rights) => next.options.rights = rights,
        Change::AllowUpload(on) => next.options.allow_upload = on,
        Change::Edit(on) => next.options.edit = on,
        Change::Comment(on) => next.options.comment = on,
        Change::AllowReshare(on) => next.options.allow_reshare = on,
        Change::Watermark(on) => next.options.watermark = on,
        Change::Notify(notify) => next.options.notify = notify,
        Change::OpenLog(on) => next.options.open_log = on,
        Change::Title(title) => next.options.title = title,
        Change::Message(message) => next.options.message = message,
        Change::Slug(slug) => next.options.slug = slug,
        Change::PairRequest(id) => {
            if next.request_id.is_some() {
                return Err(UpdateRefusal::AlreadyPaired);
            }
            next.request_id = Some(id);
        }
        Change::PairCommentLog(id) => {
            if next.comment_log.is_some() {
                return Err(UpdateRefusal::AlreadyPaired);
            }
            next.comment_log = Some(id);
        }
        Change::AddWrap(wrap) => {
            let member = wrap.member();
            if next.wraps.contains_key(&member) {
                return Err(UpdateRefusal::AlreadyWrapped);
            }
            // A person added to a `people` link joins the named people; any other audience that
            // holds wraps (`approval`) only gains the wrap, and one that holds none breaks the rule
            // of the wraps when the row is checked.
            if let Audience::People { people } = &mut next.options.audience {
                people.insert(member);
            }
            next.wraps.insert(member, wrap);
        }
        Change::Audience(to) => {
            if to != next.options.audience {
                if !next.options.audience.changes_without_rotation(&to)
                    && !azlin_seams::mutant!("links_audience_change_without_rotation")
                {
                    return Err(UpdateRefusal::RotationRequired(RotationReason::Audience));
                }
                next.options.audience = to;
            }
        }
        Change::RemoveWrap(member) => {
            if !next.wraps.contains_key(&member) {
                return Err(UpdateRefusal::NoSuchWrap);
            }
            if !azlin_seams::mutant!("links_wrap_removed_without_rotation") {
                return Err(UpdateRefusal::RotationRequired(RotationReason::WrapRemoved));
            }
            next.wraps.remove(&member);
            if let Audience::People { people } = &mut next.options.audience {
                people.remove(&member);
            }
        }
        Change::Password(record) => {
            if record != next.opaque_record {
                if !azlin_seams::mutant!("links_password_change_without_rotation") {
                    return Err(UpdateRefusal::RotationRequired(RotationReason::Password));
                }
                next.options.password = record.is_some();
                next.opaque_record = record;
            }
        }
        Change::Kind(kind) => {
            if kind != next.options.kind {
                return Err(UpdateRefusal::RotationRequired(RotationReason::Kind));
            }
        }
        Change::Subfolders(on) => {
            if on != next.options.subfolders {
                return Err(UpdateRefusal::RotationRequired(RotationReason::Subfolders));
            }
        }
        Change::Grants(grants) => {
            if grants != next.grants {
                return Err(UpdateRefusal::RotationRequired(RotationReason::Grants));
            }
        }
    }
    Ok(())
}

/// The violations of `policy` in the options of `next`, for the options in `touched` only.
fn policy_findings(
    next: &LinkRow,
    touched: &[OptionName],
    policy: &LinkPolicy,
) -> Vec<PolicyViolation> {
    let mut found = violations(&next.options, policy, next.created_at);
    found.retain(|violation| {
        violation
            .options()
            .iter()
            .any(|option| touched.contains(option))
    });
    found
}

/// Changes a link: a server option, a pairing, a wrap added (no rotation), or, refused with
/// `RotationRequired`, a change of who can decrypt (see [`rotate`]). Only the creator or a manager
/// of the link's space may; a revoked link never changes; a link the operator or the reports have
/// paused is frozen. The new row is checked whole (the rules between options, every invariant of a
/// row) and, for the options the change touches, against `policy` (the effective `link_policy` of
/// the creator's group): a forced option may not be undone, an expiry may not pass the longest life
/// (counted from `created_at`) and must lie after `now`. A change that changes nothing is `Ok`.
/// On every `Err` the record is as it was.
///
/// Implements planning/architecture.md section 5.12 ("Changing a link": "server options change at any
/// time (`link_update`)"; "who can decrypt changes only by rotating") and section 5.10 (`link_policy`
/// bounds).
///
/// Caller: LINKS-NODE (`link_update`; the policy argument is an addition to the brief's signature,
/// drift register of lane W1-S16c1: without it `update` could undo what `apply_policy` forced).
pub fn update(
    r: &mut LinkRecord,
    change: Change,
    by: &Actor,
    policy: &LinkPolicy,
    now: Stamp,
) -> Result<(), UpdateRefusal> {
    if !may_change(r, by) {
        return Err(UpdateRefusal::NotPermitted);
    }
    guard_open(r)?;
    guard_unfrozen(r)?;
    let touched = change.touches();
    let mut next = r.row().clone();
    apply(&mut next, change)?;
    if next == *r.row() {
        return Ok(());
    }
    check_row(&next).map_err(UpdateRefusal::Invalid)?;
    // A changed expiry must lie ahead, as it must when the link is made.
    if next.options.expires_at != r.row().options.expires_at
        && next.options.expires_at.is_some_and(|at| at <= now)
    {
        return Err(UpdateRefusal::Invalid(RecordError::Options(vec![
            OptionError::ExpiresAtNotInTheFuture,
        ])));
    }
    let found = policy_findings(&next, touched, policy);
    if !found.is_empty() {
        return Err(UpdateRefusal::Policy(found));
    }
    r.commit(next).map_err(UpdateRefusal::Invalid)
}

/// The parts of a link that only a rotation changes: who can decrypt. They come with the new key:
/// the manifest sealed under it, the wraps of it, the OPAQUE record of the password it derives from.
///
/// Implements planning/architecture.md section 5.12 ("who can decrypt changes only by rotating: a new
/// id and key").
///
/// Caller: `rotate`; Seam: LINKS-NODE (`link_update` with a new key), S16d (the client makes the
/// new key and seals the new manifest).
#[derive(Clone, Debug)]
pub struct KeyBound {
    /// The audience after the rotation.
    pub audience: Audience,
    /// The kind after the rotation.
    pub kind: LinkKind,
    /// The subfolders after the rotation.
    pub subfolders: bool,
    /// The AZS2 manifest sealed under the new key.
    pub manifest_object: ObjectRef,
    /// What the link may read after the rotation.
    pub grants: Grants,
    /// The OPAQUE record of the password after the rotation; `Some` makes the link a password
    /// link, `None` a link without a password.
    pub opaque_record: Option<OpaqueRecord>,
    /// The wraps of the NEW key (one for each person of a `people` audience).
    pub wraps: Vec<Wrap>,
    /// The new paired request: the old one ends with the old id (S16f's `end_with_link(Rotated)`),
    /// so giving the old id back is refused (`AlreadyPaired`); `None` for a link that needs none.
    pub request: Option<RequestId>,
}

/// What a rotation gives back: the node moves the link from the old id to the new one, and the old
/// paired request ends.
///
/// Implements planning/architecture.md section 5.12 ("a new id and key, after which the old URL stops
/// at once").
///
/// Caller: `rotate`; Seam: LINKS-NODE (it re-keys its index of ids: from this moment the old id
/// resolves nothing; it ends `ended_request` with `LinkEnded::Rotated`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Rotated {
    /// The id the link answered to until now.
    pub old: PublicLinkId,
    /// The id it answers to from now on.
    pub new: PublicLinkId,
    /// The paired request of the old id, which ends with `LinkEnded::Rotated`.
    pub ended_request: Option<RequestId>,
}

/// Rotates a link: a new public id and the key-bound parts that come with the new key; "the old URL
/// stops at once" (the old id answers nothing from now on: [`LinkRecord::answers_to`]). The counters
/// start again, `keyed_at` is `now` (never before the creation: a rotation is not refused for a
/// clock reason), `created_at`, the comment log and the pauses stay. The old paired request ends
/// and a NEW one comes with the rotation. Who may, the frozen state, the whole-row check and the
/// policy (for the audience, the password, the kind and the subfolders) are as for [`update`]. An
/// entropy failure makes no id; on every `Err` the record is as it was.
///
/// Implements planning/architecture.md section 5.12 ("who can decrypt changes only by **rotating**: a
/// new id and key, after which the old URL stops at once").
///
/// Caller: LINKS-NODE (`link_update` carrying a new key); the brief's signature `rotate(r, e, now)`
/// gains the key-bound parts, the actor and the policy (drift register of lane W1-S16c1: a rotation
/// that cannot carry the manifest sealed under the new key is a link nobody can open).
pub fn rotate(
    r: &mut LinkRecord,
    entropy: &mut dyn Entropy,
    to: KeyBound,
    by: &Actor,
    policy: &LinkPolicy,
    now: Stamp,
) -> Result<Rotated, UpdateRefusal> {
    if !may_change(r, by) {
        return Err(UpdateRefusal::NotPermitted);
    }
    guard_open(r)?;
    guard_unfrozen(r)?;
    // A paired request ends with its link (S16f): a rotation brings a NEW request. Giving the old one
    // back would end a request the link still holds.
    if to.request.is_some()
        && to.request.as_ref() == r.request_id()
        && !azlin_seams::mutant!("links_rotate_reuses_request")
    {
        return Err(UpdateRefusal::AlreadyPaired);
    }
    let old = *r.id();
    let drawn = new_id(entropy).map_err(UpdateRefusal::Entropy)?;
    let new = if azlin_seams::mutant!("links_rotate_keeps_old_id") {
        old
    } else {
        drawn
    };
    if new == old && !azlin_seams::mutant!("links_rotate_keeps_old_id") {
        return Err(UpdateRefusal::SameId);
    }
    let mut wraps = BTreeMap::new();
    for wrap in to.wraps {
        let member = wrap.member();
        if wraps.insert(member, wrap).is_some() {
            return Err(UpdateRefusal::Invalid(RecordError::DuplicateWrap(member)));
        }
    }
    let mut next = r.row().clone();
    let ended_request = next.request_id;
    next.id = new;
    // A rotation is never refused for a clock reason: a stamp before the creation (a clock that went
    // back) keys the link at its creation.
    next.keyed_at = now.max(next.created_at);
    next.options.audience = to.audience;
    next.options.kind = to.kind;
    next.options.subfolders = to.subfolders;
    next.options.password = to.opaque_record.is_some();
    next.opaque_record = to.opaque_record;
    next.manifest_object = to.manifest_object;
    next.grants = to.grants;
    next.wraps = wraps;
    next.request_id = to.request;
    next.counters = Counters::zero();
    check_row(&next).map_err(UpdateRefusal::Invalid)?;
    let touched = [
        OptionName::Audience,
        OptionName::Password,
        OptionName::Kind,
        OptionName::Subfolders,
    ];
    let found = policy_findings(&next, &touched, policy);
    if !found.is_empty() {
        return Err(UpdateRefusal::Policy(found));
    }
    r.commit(next).map_err(UpdateRefusal::Invalid)?;
    Ok(Rotated {
        old,
        new,
        ended_request,
    })
}

/// Pauses a link in the name of `by`: the creator or a manager (the owner's pause), the reports, or
/// the operator. Each source has a pause of its own, so pausing never loses another source's pause
/// ("`paused` stops and resumes a link without changing it": nothing else of the record changes).
/// Pausing a paused link again is `Ok`. Returns the pauses now in force.
///
/// Implements planning/architecture.md section 5.12 ("`paused` stops and resumes a link without
/// changing it") and section 3.9 ("a link reported from three different devices pauses until
/// decided").
///
/// Caller: LINKS-NODE (`link_update` pause), S46 (`Actor::Reports`: the report counting pauses a link
/// through the node), AzCtl's operator rows.
pub fn pause(r: &mut LinkRecord, by: &Actor) -> Result<Pauses, UpdateRefusal> {
    let source = match by {
        Actor::Member { .. } if may_change(r, by) => PausedBy::Owner,
        Actor::Member { .. } => return Err(UpdateRefusal::NotPermitted),
        Actor::Reports => PausedBy::Reports,
        Actor::Operator => PausedBy::Operator,
    };
    guard_open(r)?;
    let next = r.pauses().with(source);
    r.set_pauses(next);
    Ok(next)
}

/// Lifts the pauses `by` may lift: the creator or a manager lifts the owner's pause, the operator
/// lifts its own and the reports' ("pauses until the operator decides"); the reports lift nothing. A
/// pause of a source `by` may not lift stays: only the operator lifts the operator's pause. Returns
/// the pauses still in force (the link runs again when none is).
///
/// Implements planning/architecture.md section 5.12 ("`paused` stops and resumes a link") and section
/// 3.9 ("pauses until decided").
///
/// Caller: LINKS-NODE (`link_update` unpause), AzCtl's operator rows.
pub fn unpause(r: &mut LinkRecord, by: &Actor) -> Result<Pauses, UpdateRefusal> {
    let lifts: &[PausedBy] = match by {
        Actor::Member { .. } if may_change(r, by) => &[PausedBy::Owner],
        Actor::Operator => &[PausedBy::Operator, PausedBy::Reports],
        Actor::Member { .. } | Actor::Reports => return Err(UpdateRefusal::NotPermitted),
    };
    guard_open(r)?;
    let mut next = r.pauses();
    if azlin_seams::mutant!("links_operator_pause_lifted_by_owner") {
        next = Pauses::NONE;
    } else {
        for source in lifts {
            next = next.without(*source);
        }
    }
    r.set_pauses(next);
    Ok(next)
}

/// Revokes a link, for good: the creator, a manager or the operator. Nothing changes a revoked link
/// afterwards (every function here answers `Revoked`), and revoking again keeps the first time. A
/// revocation is never refused for a clock reason: the stamp is stored as given.
///
/// Implements planning/architecture.md section 5.12 ("`link_revoke` ends it").
///
/// Caller: LINKS-NODE (`link_revoke`), AzCtl's operator rows (a takedown).
pub fn revoke(r: &mut LinkRecord, by: &Actor, now: Stamp) -> Result<(), UpdateRefusal> {
    if !(matches!(by, Actor::Operator) || may_change(r, by)) {
        return Err(UpdateRefusal::NotPermitted);
    }
    r.mark_revoked(now);
    Ok(())
}

/// Re-points a live folder link at its new version: the new manifest object (with the new chunk
/// keys) and the grants that read it. Only a writer of the space may ("writers re-point the link at
/// each new version and add the new chunk keys to its manifest"); only a `folder_live` link is
/// re-pointed (every other kind is a rotation); a revoked or frozen link is not. The brief's
/// `repoint(r, new_manifest, by)` gains the grants: the node cannot read the manifest, so the new
/// version's objects are served only if the grants name them (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 5.12 ("Live folders (`folder_live`): writers re-point
/// the link at each new version and add the new chunk keys to its manifest").
///
/// Caller: LINKS-NODE (`link_update` repoint, sent by a writer's device when the folder changes).
pub fn repoint(
    r: &mut LinkRecord,
    manifest_object: ObjectRef,
    grants: Grants,
    by: &Actor,
) -> Result<(), UpdateRefusal> {
    let writer = match by {
        Actor::Member { write, .. } => {
            *write || azlin_seams::mutant!("links_repoint_by_non_writer")
        }
        Actor::Reports | Actor::Operator => false,
    };
    if !writer {
        return Err(UpdateRefusal::NotPermitted);
    }
    guard_open(r)?;
    guard_unfrozen(r)?;
    if r.kind() != LinkKind::FolderLive && !azlin_seams::mutant!("links_repoint_any_kind") {
        return Err(UpdateRefusal::NotLive);
    }
    let mut next = r.row().clone();
    next.manifest_object = manifest_object;
    next.grants = grants;
    r.commit(next).map_err(UpdateRefusal::Invalid)
}
