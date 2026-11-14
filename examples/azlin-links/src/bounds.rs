//! The bounds of the account: what a `link_policy` lets a member make of a link
//! (planning/architecture.md section 5.12 "Admin bounds": "`link_policy` (5.10) decides which
//! options members may use and forces some; groups only tighten. A workspace can switch public links
//! off for its members").
//!
//! The policy is `azlin_policy::LinkPolicy` (S17b), already the effective policy of the member's
//! group (`azlin_policy::effective`): this module only checks a link against it.
//!
//! - **refuses**: an audience the policy does not allow (a workspace that switches public links
//!   off allows every audience but `anyone`), `anyone` without a password where the policy asks
//!   one, an expiry beyond the longest the policy allows (or no expiry where one is asked), and
//!   `allow_upload`, `edit`, `comment`, `open_log` or a `slug` where the policy does not allow them;
//! - **forces**: `preview_only`, the watermark and `app_only` where the policy forces them. A forced
//!   option is not an error at creation: the node sets it ([`Bounded::forced`] says which it set,
//!   so the dialog can show "set by your administrator"). When a link is CHANGED, a change that
//!   would undo a forced option is refused (`PolicyViolation::Forced`) instead of silently kept.
//!
//! `max_expiry` is "the longest time a link may live (from its creation to `expires_at`)": with a
//! longest time the link must have an `expires_at`, and it must be no later than the creation plus
//! the longest time.
//!
//! Implements planning/architecture.md section 5.12 ("Admin bounds") and section 5.10
//! (`link_policy`).

use azlin_policy::{AudienceClass, LinkPolicy};
use azlin_seams::{Span, Stamp};

use crate::options::{LinkOptions, Rights};
use crate::strength::OptionName;
use crate::validate::{Facts, Validated};

/// The options of a link that pass the rules between options AND the account's `link_policy`, with
/// the options the policy forced already set. A witness: private fields, no `Clone`, `Default` or
/// `From`; only [`apply_policy`] makes one, and `LinkRecord::create` consumes it.
///
/// Implements planning/architecture.md section 5.12 ("Admin bounds").
///
/// Caller: `LinkRecord::create`; Seam: LINKS-NODE (`link_create`).
#[derive(Debug)]
pub struct Bounded {
    options: LinkOptions,
    facts: Facts,
    forced: Vec<OptionName>,
}

impl Bounded {
    /// The options, the forced ones set.
    pub fn options(&self) -> &LinkOptions {
        &self.options
    }

    /// The facts the options were checked against.
    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    /// The options the policy set (`rights`, `watermark`, `app_only`) that the creator had not set.
    pub fn forced(&self) -> &[OptionName] {
        &self.forced
    }

    /// The options, the facts and the forced names, for `LinkRecord::create`.
    pub(crate) fn into_parts(self) -> (LinkOptions, Facts, Vec<OptionName>) {
        (self.options, self.facts, self.forced)
    }
}

/// A rule of the account's `link_policy` that a link breaks.
///
/// Implements planning/architecture.md section 5.10 (`link_policy`) and section 5.12 ("Admin
/// bounds").
///
/// Caller: `apply_policy`, `UpdateRefusal::Policy`; Seam: the share dialog (it shows which rule and
/// of which option).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum PolicyViolation {
    /// The policy does not allow this audience (a workspace can switch public links off).
    #[error("the account's policy does not allow `{0}` links")]
    AudienceNotAllowed(AudienceClass),
    /// The policy asks a password for links to anyone.
    #[error("the account's policy asks a password for links to anyone")]
    PasswordRequiredForAnyone,
    /// The policy gives a link a longest life and the link has no `expires_at`.
    #[error("the account's policy asks every link to expire within {} seconds", .longest.as_secs())]
    ExpiryRequired {
        /// The longest life the policy allows.
        longest: Span,
    },
    /// The link's `expires_at` is beyond the longest life the policy allows.
    #[error("the account's policy ends every link by {}", .latest.as_secs())]
    ExpiryTooLate {
        /// The latest `expires_at` the policy allows for this link.
        latest: Stamp,
    },
    /// The policy does not allow uploads through a link.
    #[error("the account's policy does not allow uploads through a link")]
    UploadsNotAllowed,
    /// The policy does not allow `edit`.
    #[error("the account's policy does not allow edit through a link")]
    EditNotAllowed,
    /// The policy does not allow comments.
    #[error("the account's policy does not allow comments on a link")]
    CommentsNotAllowed,
    /// The policy does not allow an open log.
    #[error("the account's policy does not allow an open log")]
    OpenLogsNotAllowed,
    /// The policy does not allow a slug.
    #[error("the account's policy does not allow a slug")]
    SlugsNotAllowed,
    /// The policy forces this option; a change may not undo it.
    #[error("the account's policy forces `{0}`")]
    Forced(OptionName),
}

impl PolicyViolation {
    /// The options the violation is about (a change is judged by the violations of the options it
    /// touches).
    ///
    /// Implements planning/architecture.md section 5.10 (`link_policy`).
    pub fn options(&self) -> Vec<OptionName> {
        match self {
            PolicyViolation::AudienceNotAllowed(_) => vec![OptionName::Audience],
            PolicyViolation::PasswordRequiredForAnyone => {
                vec![OptionName::Audience, OptionName::Password]
            }
            PolicyViolation::ExpiryRequired { .. } | PolicyViolation::ExpiryTooLate { .. } => {
                vec![OptionName::ExpiresAt]
            }
            PolicyViolation::UploadsNotAllowed => vec![OptionName::AllowUpload],
            PolicyViolation::EditNotAllowed => vec![OptionName::Edit],
            PolicyViolation::CommentsNotAllowed => vec![OptionName::Comment],
            PolicyViolation::OpenLogsNotAllowed => vec![OptionName::OpenLog],
            PolicyViolation::SlugsNotAllowed => vec![OptionName::Slug],
            PolicyViolation::Forced(option) => vec![*option],
        }
    }
}

/// The three options a policy can force.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Forcing {
    /// `force_preview_only`: the link is `preview_only`.
    PreviewOnly,
    /// `force_watermark`: the link carries the watermark.
    Watermark,
    /// `force_app_only`: the link is `app_only`.
    AppOnly,
}

impl Forcing {
    /// The option that is forced.
    fn option(self) -> OptionName {
        match self {
            Forcing::PreviewOnly => OptionName::Rights,
            Forcing::Watermark => OptionName::Watermark,
            Forcing::AppOnly => OptionName::AppOnly,
        }
    }
}

/// The options the policy forces that `o` does not have yet.
fn forcings(o: &LinkOptions, p: &LinkPolicy) -> Vec<Forcing> {
    let mut forced = Vec::new();
    if p.force_preview_only
        && o.rights != Rights::PreviewOnly
        && !azlin_seams::mutant!("links_policy_forcing_preview_only_skipped")
    {
        forced.push(Forcing::PreviewOnly);
    }
    if p.force_watermark
        && !o.watermark
        && !azlin_seams::mutant!("links_policy_forcing_watermark_skipped")
    {
        forced.push(Forcing::Watermark);
    }
    if p.force_app_only
        && !o.app_only
        && !azlin_seams::mutant!("links_policy_forcing_app_only_skipped")
    {
        forced.push(Forcing::AppOnly);
    }
    forced
}

/// The rules of the policy that REFUSE a link (the forced options are not among them), for a link
/// created at `created_at`.
pub(crate) fn refusals(o: &LinkOptions, p: &LinkPolicy, created_at: Stamp) -> Vec<PolicyViolation> {
    let mut violations = Vec::new();
    let class = o.audience.class();
    if !p.audiences.contains(class) && !azlin_seams::mutant!("links_policy_audience_unchecked") {
        violations.push(PolicyViolation::AudienceNotAllowed(class));
    }
    if p.password_for_anyone
        && class == AudienceClass::Anyone
        && !o.password
        && !azlin_seams::mutant!("links_policy_anyone_without_password")
    {
        violations.push(PolicyViolation::PasswordRequiredForAnyone);
    }
    if let Some(longest) = p.max_expiry {
        if !azlin_seams::mutant!("links_policy_max_expiry_unchecked") {
            match o.expires_at {
                None => violations.push(PolicyViolation::ExpiryRequired { longest }),
                Some(expires_at) => {
                    // A latest stamp beyond the largest stamp (the sum overflows) bounds nothing.
                    if let Some(latest) = created_at.checked_add(longest) {
                        if expires_at > latest {
                            violations.push(PolicyViolation::ExpiryTooLate { latest });
                        }
                    }
                }
            }
        }
    }
    if !p.uploads && o.allow_upload {
        violations.push(PolicyViolation::UploadsNotAllowed);
    }
    if !p.edit && o.edit {
        violations.push(PolicyViolation::EditNotAllowed);
    }
    if !p.comments && o.comment {
        violations.push(PolicyViolation::CommentsNotAllowed);
    }
    if !p.open_logs && o.open_log {
        violations.push(PolicyViolation::OpenLogsNotAllowed);
    }
    if !p.slugs && o.slug.is_some() {
        violations.push(PolicyViolation::SlugsNotAllowed);
    }
    violations
}

/// Every violation the policy finds in a link that is being CHANGED: the refusals, and each forced
/// option the link does not have as `Forced`.
pub(crate) fn violations(
    o: &LinkOptions,
    p: &LinkPolicy,
    created_at: Stamp,
) -> Vec<PolicyViolation> {
    let mut found = refusals(o, p, created_at);
    found.extend(
        forcings(o, p)
            .into_iter()
            .map(|forcing| PolicyViolation::Forced(forcing.option())),
    );
    found
}

/// Checks a validated link against the account's `link_policy`: refuses what the policy does not
/// allow (`Err` lists EVERY violation, never an empty list) and sets what it forces (`preview_only`,
/// the watermark, `app_only`). `now` is the creation time: `max_expiry` counts from it.
///
/// Implements planning/architecture.md section 5.12 ("Admin bounds": `link_policy` "decides which
/// options members may use and forces some") and section 5.10 (`link_policy`).
///
/// Caller: LINKS-NODE (`link_create`: the policy is the effective policy of the creating member's
/// group, `azlin_policy::effective`), the tests of this crate.
pub fn apply_policy(
    v: Validated,
    p: &LinkPolicy,
    now: Stamp,
) -> Result<Bounded, Vec<PolicyViolation>> {
    let (mut options, facts) = v.into_parts();
    let violations = refusals(&options, p, now);
    if !violations.is_empty() {
        return Err(violations);
    }
    let forcings = forcings(&options, p);
    for forcing in &forcings {
        match forcing {
            Forcing::PreviewOnly => options.rights = Rights::PreviewOnly,
            Forcing::Watermark => options.watermark = true,
            Forcing::AppOnly => options.app_only = true,
        }
    }
    let forced = forcings.into_iter().map(Forcing::option).collect();
    Ok(Bounded {
        options,
        facts,
        forced,
    })
}
