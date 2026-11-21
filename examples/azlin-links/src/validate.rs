//! The rules between options: what planning/architecture.md section 5.12 says about how the options of
//! a link depend on each other, as one pure function over the options and the facts about the rest of
//! the link (`validate`). Every rule is a variant of [`OptionError`] and one test; `validate` collects
//! ALL the errors, so the share dialog can show them together, and every error names the option it is
//! about ([`OptionError::option`]).
//!
//! | rule | the plan says |
//! |---|---|
//! | slug needs a password or a named audience | "`slug` ... only with a password or a named audience, because a name can be guessed" |
//! | edit needs a named audience | "`edit` | named audiences only" |
//! | comment needs a signed-in audience | "`comment` | signed-in audiences" |
//! | open log needs a signed-in audience | "`open_log` | signed-in audiences only, off by default" |
//! | upload needs the paired request | "`allow_upload` | recipients add files through a paired request" |
//! | approval needs the paired request | "the recipient's app sends a sealed request ... through the link's paired request" |
//! | people need one wrap each | "`people` ... the share key is wrapped to each one's member key" |
//! | subfolders only for folders | "`subfolders` | with or without" (a folder has them) |
//! | every kind but a live folder names objects | "`grants` (the objects and byte ranges it may read)" |
//! | `not_before` before `expires_at` | "`not_before`, `expires_at` | dates" |
//! | counts and durations are positive | "`max_downloads`, `bytes_cap` | counts, bytes" |
//! | title and message are sealed and capped | "sealed under the link key" |
//!
//! The facts ([`Facts`]) are what the options depend on and the options cannot say: whether the link
//! has a paired request (S16f), a comment log (S16a) and the OPAQUE record of its password (S16e),
//! how many wraps of the share key and how many objects or ranges it holds, and the time. The
//! brief's `validate(o, paired_request, wraps, now)` has the first, the wraps and the time; the
//! rest are the facts three of its own named checks (`selection_needs_objects`, the password, the
//! comment log) need (drift register of lane W1-S16c1).
//!
//! Implements planning/architecture.md section 5.12 (the options table, "Passwords with OPAQUE",
//! "Asking for access").

use azlin_seams::{ByteCount, Span, Stamp};

use crate::audience::{Audience, MAX_GROUPS, MAX_PEOPLE};
use crate::kind::LinkKind;
use crate::options::{
    LinkOptions, MAX_EXPIRES_AFTER_OPEN, MAX_IDLE_EXPIRY, MESSAGE_MAX_LEN, TITLE_MAX_LEN,
};
use crate::record::{MAX_GRANTS, MAX_WRAPS};
use crate::strength::OptionName;

/// What the options of a link depend on and cannot say themselves: the other parts of the link and
/// the time.
///
/// Implements planning/architecture.md section 5.12 (`reg_link` holds the paired request, the comment
/// log, the OPAQUE record, the wraps and the grants beside the options).
///
/// Caller: `validate`, `LinkRecord::create` (it checks the parts it is given are these);
/// Seam: LINKS-NODE (`link_create` states them from the command).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Facts {
    /// The link has a paired request (S16f's `reg_request`): `allow_upload` and `approval` need it.
    pub paired_request: bool,
    /// The link has a comment log (an append log, 5.13): `comment` needs it.
    pub comment_log: bool,
    /// The link holds the OPAQUE record of its password: `password` and the record go together.
    pub opaque_record: bool,
    /// How many wraps of the share key the link holds.
    pub wraps: usize,
    /// How many objects or byte ranges the link may read (its `grants`).
    pub grants: usize,
    /// The leader's stamp of the check.
    pub now: Stamp,
}

/// The options of a link that pass every rule between options, together with the facts they were
/// checked against. A witness: its fields are private, it has no `Clone`, `Default` or `From`; only
/// [`validate`] makes one, and `apply_policy` consumes it.
///
/// Implements planning/architecture.md section 5.12 (the rules between options).
///
/// Caller: `apply_policy`; Seam: LINKS-NODE (`link_create`).
#[derive(Debug)]
pub struct Validated {
    options: LinkOptions,
    facts: Facts,
}

impl Validated {
    /// The options that were checked.
    pub fn options(&self) -> &LinkOptions {
        &self.options
    }

    /// The facts they were checked against.
    pub fn facts(&self) -> &Facts {
        &self.facts
    }

    /// The options and the facts, for `apply_policy`.
    pub(crate) fn into_parts(self) -> (LinkOptions, Facts) {
        (self.options, self.facts)
    }
}

/// A rule between options that the options break. Each variant is one rule (or one limit); the
/// option it is about is [`OptionError::option`].
///
/// Implements planning/architecture.md section 5.12 (the options table and its rules).
///
/// Caller: `validate`, `RecordError::Options`, `UpdateRefusal`; Seam: the share dialog (it shows the
/// reason next to the option).
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum OptionError {
    /// A `people` audience that names no one.
    #[error("a `people` audience names at least one person")]
    PeopleEmpty,
    /// More people than the limit.
    #[error("a link names at most {} people, not {people}", MAX_PEOPLE)]
    TooManyPeople {
        /// The number named.
        people: usize,
    },
    /// More groups than the limit.
    #[error("a link names at most {} groups, not {groups}", MAX_GROUPS)]
    TooManyGroups {
        /// The number named.
        groups: usize,
    },
    /// A `people` audience whose wraps are not one for each person.
    #[error("{people} people need one wrap of the share key each, not {wraps}")]
    PeopleNeedOneWrapEach {
        /// The number of people named.
        people: usize,
        /// The number of wraps the link holds.
        wraps: usize,
    },
    /// Wraps on a link whose audience has none (the key travels in the URL's fragment).
    #[error("only `people` and `approval` links hold wraps of the share key, not {wraps}")]
    WrapsNotAllowed {
        /// The number of wraps.
        wraps: usize,
    },
    /// More wraps than the limit (an `approval` link gains one at each approval).
    #[error("a link holds at most {} wraps, not {wraps}", MAX_WRAPS)]
    TooManyWraps {
        /// The number of wraps.
        wraps: usize,
    },
    /// An `approval` link without the paired request that carries the asks.
    #[error("an `approval` link needs the paired request")]
    ApprovalNeedsPairedRequest,
    /// `password` without the OPAQUE record of the password.
    #[error("a password link holds the OPAQUE record of the password")]
    PasswordNeedsRecord,
    /// An OPAQUE record on a link without a password.
    #[error("a link without a password holds no OPAQUE record")]
    RecordWithoutPassword,
    /// A slug without a password and without a named audience: a name can be guessed.
    #[error("a slug needs a password or a named audience: a name can be guessed")]
    SlugNeedsPasswordOrNamedAudience,
    /// `edit` for an audience that is not named.
    #[error("edit needs a named audience")]
    EditNeedsNamedAudience,
    /// `comment` for an audience that is not signed in.
    #[error("comments need a signed-in audience")]
    CommentNeedsSignedInAudience,
    /// `comment` without the link's comment log.
    #[error("comments need the link's comment log")]
    CommentNeedsLog,
    /// `open_log` for an audience that is not signed in.
    #[error("an open log needs a signed-in audience")]
    OpenLogNeedsSignedInAudience,
    /// `allow_upload` without the paired request.
    #[error("uploads need the paired request")]
    UploadNeedsPairedRequest,
    /// `subfolders` on a kind that is no folder.
    #[error("subfolders are a choice of the folder kinds, not of `{kind}`")]
    SubfoldersOnlyForFolders {
        /// The kind of the link.
        kind: LinkKind,
    },
    /// A kind that needs objects, with none.
    #[error("a `{kind}` link names at least one object")]
    NeedsObjects {
        /// The kind of the link.
        kind: LinkKind,
    },
    /// More objects or ranges than the limit.
    #[error("a link reads at most {} objects or ranges, not {grants}", MAX_GRANTS)]
    TooManyGrants {
        /// The number of grants.
        grants: usize,
    },
    /// `not_before` is not before `expires_at`: the link would never be open.
    #[error("`not_before` is not before `expires_at`: the link would never be open")]
    EmptyWindow,
    /// `expires_at` is not after the time of the check.
    #[error("`expires_at` is not in the future")]
    ExpiresAtNotInTheFuture,
    /// A count or a duration of zero.
    #[error("`{option}` is a positive number")]
    NotPositive {
        /// The option.
        option: OptionName,
    },
    /// A duration beyond its limit.
    #[error("`{option}` is longer than its limit")]
    TooLong {
        /// The option.
        option: OptionName,
    },
    /// A sealed title beyond its cap.
    #[error("the sealed title is longer than {} bytes", TITLE_MAX_LEN)]
    TitleTooLong,
    /// A sealed message beyond its cap.
    #[error("the sealed message is longer than {} bytes", MESSAGE_MAX_LEN)]
    MessageTooLong,
}

impl OptionError {
    /// The option the error is about.
    ///
    /// Implements planning/architecture.md section 5.12 (the options table).
    pub fn option(&self) -> OptionName {
        match self {
            OptionError::PeopleEmpty
            | OptionError::TooManyPeople { .. }
            | OptionError::TooManyGroups { .. }
            | OptionError::PeopleNeedOneWrapEach { .. }
            | OptionError::WrapsNotAllowed { .. }
            | OptionError::TooManyWraps { .. }
            | OptionError::ApprovalNeedsPairedRequest => OptionName::Audience,
            OptionError::PasswordNeedsRecord | OptionError::RecordWithoutPassword => {
                OptionName::Password
            }
            OptionError::SlugNeedsPasswordOrNamedAudience => OptionName::Slug,
            OptionError::EditNeedsNamedAudience => OptionName::Edit,
            OptionError::CommentNeedsSignedInAudience | OptionError::CommentNeedsLog => {
                OptionName::Comment
            }
            OptionError::OpenLogNeedsSignedInAudience => OptionName::OpenLog,
            OptionError::UploadNeedsPairedRequest => OptionName::AllowUpload,
            OptionError::SubfoldersOnlyForFolders { .. } => OptionName::Subfolders,
            OptionError::NeedsObjects { .. } | OptionError::TooManyGrants { .. } => {
                OptionName::Kind
            }
            OptionError::EmptyWindow => OptionName::NotBefore,
            OptionError::ExpiresAtNotInTheFuture => OptionName::ExpiresAt,
            OptionError::NotPositive { option } | OptionError::TooLong { option } => *option,
            OptionError::TitleTooLong => OptionName::Title,
            OptionError::MessageTooLong => OptionName::Message,
        }
    }
}

/// The rules that hold for every link at every moment (they do not look at the clock): the same
/// function checks the options of a link that is being made, of one that is being changed and of a
/// row that is being read back. Errors come in the order of the rules.
pub(crate) fn rules(o: &LinkOptions, f: &Facts) -> Vec<OptionError> {
    let mut errors = Vec::new();

    // who: the audience and the wraps of the share key.
    match &o.audience {
        Audience::People { people } => {
            if people.is_empty() {
                errors.push(OptionError::PeopleEmpty);
            }
            if people.len() > MAX_PEOPLE {
                errors.push(OptionError::TooManyPeople {
                    people: people.len(),
                });
            }
            if f.wraps != people.len() && !azlin_seams::mutant!("links_people_wraps_not_counted") {
                errors.push(OptionError::PeopleNeedOneWrapEach {
                    people: people.len(),
                    wraps: f.wraps,
                });
            }
        }
        Audience::Members { groups } => {
            if groups.len() > MAX_GROUPS {
                errors.push(OptionError::TooManyGroups {
                    groups: groups.len(),
                });
            }
            if f.wraps > 0 {
                errors.push(OptionError::WrapsNotAllowed { wraps: f.wraps });
            }
        }
        Audience::Approval => {
            if !f.paired_request {
                errors.push(OptionError::ApprovalNeedsPairedRequest);
            }
            if f.wraps > MAX_WRAPS {
                errors.push(OptionError::TooManyWraps { wraps: f.wraps });
            }
        }
        Audience::Anyone | Audience::SignedIn => {
            if f.wraps > 0 {
                errors.push(OptionError::WrapsNotAllowed { wraps: f.wraps });
            }
        }
    }

    // the password and its OPAQUE record go together.
    if o.password && !f.opaque_record {
        errors.push(OptionError::PasswordNeedsRecord);
    }
    if !o.password && f.opaque_record {
        errors.push(OptionError::RecordWithoutPassword);
    }

    // a name can be guessed; edit is for named audiences; comments and the open log are for signed-in
    // audiences.
    if o.slug.is_some()
        && !(o.password || o.audience.is_named())
        && !azlin_seams::mutant!("links_slug_without_password_or_name")
    {
        errors.push(OptionError::SlugNeedsPasswordOrNamedAudience);
    }
    if o.edit && !o.audience.is_named() && !azlin_seams::mutant!("links_edit_for_unnamed_audience")
    {
        errors.push(OptionError::EditNeedsNamedAudience);
    }
    if o.comment {
        if !o.audience.is_signed_in() && !azlin_seams::mutant!("links_comment_for_anonymous") {
            errors.push(OptionError::CommentNeedsSignedInAudience);
        }
        if !f.comment_log {
            errors.push(OptionError::CommentNeedsLog);
        }
    }
    if o.open_log
        && !o.audience.is_signed_in()
        && !azlin_seams::mutant!("links_open_log_for_anonymous")
    {
        errors.push(OptionError::OpenLogNeedsSignedInAudience);
    }

    // uploads go through the paired request.
    if o.allow_upload
        && !f.paired_request
        && !azlin_seams::mutant!("links_upload_without_paired_request")
    {
        errors.push(OptionError::UploadNeedsPairedRequest);
    }

    // what: subfolders belong to folders; a link serves something.
    if o.subfolders && !o.kind.is_folder() {
        errors.push(OptionError::SubfoldersOnlyForFolders { kind: o.kind });
    }
    if f.grants == 0 && o.kind.needs_objects() {
        errors.push(OptionError::NeedsObjects { kind: o.kind });
    }
    if f.grants > MAX_GRANTS {
        errors.push(OptionError::TooManyGrants { grants: f.grants });
    }

    // when: the window of dates is not empty.
    if let (Some(not_before), Some(expires_at)) = (o.not_before, o.expires_at) {
        if not_before >= expires_at {
            errors.push(OptionError::EmptyWindow);
        }
    }

    // how much: counts and durations are positive, and durations are bounded.
    if o.max_downloads == Some(0) {
        errors.push(OptionError::NotPositive {
            option: OptionName::MaxDownloads,
        });
    }
    if o.bytes_cap == Some(ByteCount::ZERO) {
        errors.push(OptionError::NotPositive {
            option: OptionName::BytesCap,
        });
    }
    if o.max_devices == Some(0) {
        errors.push(OptionError::NotPositive {
            option: OptionName::MaxDevices,
        });
    }
    if o.expires_after_open == Some(Span::ZERO) {
        errors.push(OptionError::NotPositive {
            option: OptionName::ExpiresAfterOpen,
        });
    }
    if o.idle_expiry == Some(Span::ZERO) {
        errors.push(OptionError::NotPositive {
            option: OptionName::IdleExpiry,
        });
    }
    if o.expires_after_open
        .is_some_and(|span| span > MAX_EXPIRES_AFTER_OPEN)
    {
        errors.push(OptionError::TooLong {
            option: OptionName::ExpiresAfterOpen,
        });
    }
    if o.idle_expiry.is_some_and(|span| span > MAX_IDLE_EXPIRY) {
        errors.push(OptionError::TooLong {
            option: OptionName::IdleExpiry,
        });
    }

    // presentation: sealed and capped.
    if o.title
        .as_ref()
        .is_some_and(|title| title.len() > TITLE_MAX_LEN)
    {
        errors.push(OptionError::TitleTooLong);
    }
    if o.message
        .as_ref()
        .is_some_and(|message| message.len() > MESSAGE_MAX_LEN)
    {
        errors.push(OptionError::MessageTooLong);
    }

    errors
}

/// Checks the options of a link that is being made against every rule between options and the
/// facts about the rest of the link. `Ok` carries the witness `apply_policy` takes; `Err` lists
/// EVERY rule that is broken (never an empty list), in the order of the rules.
///
/// Implements planning/architecture.md section 5.12 (the options table: slug, edit, comment,
/// open_log, allow_upload, audience, kind, subfolders, the dates, the counts, title and message).
///
/// Caller: LINKS-NODE (`link_create`, after S9's command decoding), the tests of this crate;
/// Seam: S16d (the share dialog may run it before it asks).
pub fn validate(o: &LinkOptions, f: &Facts) -> Result<Validated, Vec<OptionError>> {
    let mut errors = rules(o, f);
    if o.expires_at.is_some_and(|expires_at| expires_at <= f.now) {
        errors.push(OptionError::ExpiresAtNotInTheFuture);
    }
    if errors.is_empty() {
        Ok(Validated {
            options: o.clone(),
            facts: *f,
        })
    } else {
        Err(errors)
    }
}
