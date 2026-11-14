//! Azlin links: the server side of links, in pure logic (planning/architecture.md section 5.12
//! "Sharing: links, requests, external members"; build unit S16, lanes W1-S16c1 and W1-S16c2).
//!
//! This crate decides everything the nodes decide about a link. Its first lane (S16c1, this one)
//! holds what a link IS and how it may CHANGE; its second lane (S16c2) holds how it is RESOLVED and
//! COUNTED.
//!
//! | module | what | lane |
//! |---|---|---|
//! | [`options`] | `LinkOptions`: every row of the table of 5.12; `Rights`, `Notify`, `Sealed` | S16c1 |
//! | [`audience`] | `Audience`: anyone, signed in, members, people, approval | S16c1 |
//! | [`kind`] | `LinkKind`: file, selection, folder snapshot, folder live, version | S16c1 |
//! | [`strength`] | `Strength`, `OptionName`, `STRENGTH`: how each option is enforced, "the apps show it next to the option" | S16c1 |
//! | `validate` | `validate`: the rules between options | S16c1 |
//! | [`bounds`] | `apply_policy`: the account's `link_policy` refuses some options and forces others | S16c1 |
//! | [`record`] | `LinkRecord` (`reg_link`), `LinkRow`, the counters, the wraps, the grants | S16c1 |
//! | `update` | `update`, `rotate`, `pause`, `unpause`, `revoke`, `repoint`: changing a link | S16c1 |
//! | [`id`] | `PublicLinkId` (60 random bits, 12 base32 characters) and `Slug` | S16c1 |
//! | `resolve`, `counters`, `credential`, `misses`, `open_log`, `notify` | `link_resolve`, the human step, the 15-minute credential, the miss budget, the open log | S16c2 |
//!
//! **How a link is made**: `validate` (the rules between options, collecting every error) gives a
//! `Validated`; `apply_policy` (the account's `link_policy`: audiences, the longest life, a password
//! for `anyone`, forced preview only, watermark and app only, no uploads, edit, comments, open logs
//! or slugs) turns it into a `Bounded`, with the forced options set; `LinkRecord::create` takes the
//! `Bounded` and the parts of the link (the manifest, the grants, the wraps, the paired request, the
//! comment log, the OPAQUE record) and checks that they are the ones the options were checked
//! against. The two witnesses have private fields and no `Clone`, so a record cannot be made from
//! options nobody checked.
//!
//! **How a link changes**: `update` for the server options (any time), pairings and added wraps;
//! `rotate` for what changes who can decrypt (a new id and key: "the old URL stops at once");
//! `pause`, `unpause` and `revoke`; `repoint` for a live folder. Every change is judged whole and a
//! refused change leaves the record as it was.
//!
//! **What is NOT here**: the human step and the counters' functions (S16c2), the OPAQUE login
//! (S16e), the URL, the key and the sender's choices (S16d), the paired request (S16f), the comment
//! log (S16a), the objects and the manifest (S14, opaque here), the node's indexes of ids and slugs
//! and the persistence of the row (S9). No I/O, no clock (every function takes a `Stamp`), no
//! address type of any kind: a device is counted by a random token it keeps, hashed with the link's
//! id, never by an address.
//!
//! **Strengths** (5.12): `Strength::Key` is enforced by cryptography, `Server` by the nodes' refusal
//! to serve, `App` by the apps. `STRENGTH` is the table; the doc-table tests parse the plan and fail
//! when a row, a name or a strength differs.
//!
//! ```
//! use azlin_links::{
//!     apply_policy, validate, Audience, Facts, Grant, Grants, LinkKind, LinkOptions, LinkRecord,
//!     NewLink, ObjectRef, PublicLinkId, Slug,
//! };
//! use azlin_policy::LinkPolicy;
//! use azlin_seams::{Hash32, MemberId, SpaceId, Stamp};
//!
//! let now = Stamp::from_secs(1_800_000_000);
//! let options = LinkOptions::new(Audience::SignedIn, LinkKind::File);
//! let facts = Facts {
//!     paired_request: false,
//!     comment_log: false,
//!     opaque_record: false,
//!     wraps: 0,
//!     grants: 1,
//!     now,
//! };
//! let validated = validate(&options, &facts).unwrap();
//! let bounded = apply_policy(validated, &LinkPolicy::allow_all(), now).unwrap();
//! let object = ObjectRef::from_hash(Hash32::from_bytes([3; 32]));
//! let new = NewLink {
//!     space: SpaceId::from_bytes([1; 16]),
//!     created_by: MemberId::from_bytes([2; 16]),
//!     manifest_object: object,
//!     grants: Grants::new(vec![Grant { object, range: None }]).unwrap(),
//!     opaque_record: None,
//!     wraps: Vec::new(),
//!     request_id: None,
//!     comment_log: None,
//! };
//! let id: PublicLinkId = "abcdefghijkl".parse().unwrap();
//! let link = LinkRecord::create(bounded, new, id, now).unwrap();
//! assert!(link.answers_to(&id) && !link.is_paused() && !link.follow_latest());
//!
//! // A slug is a name, never an id.
//! assert!(Slug::parse("quarterly-report").is_ok());
//! assert!(Slug::parse("abcdefghijkl").is_err());
//! ```
//!
//! Implements planning/architecture.md section 5.12 (links), section 5.10 (`link_policy`), section
//! 5.15 (M7: counters only) and section 12 (E5).

#![forbid(unsafe_code)]
#![cfg_attr(
    not(test),
    deny(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::unreachable,
        clippy::todo,
        clippy::unimplemented,
        clippy::indexing_slicing
    )
)]
#![warn(missing_docs)]

/// The `ALL`, `as_str`, `Display` and strict `FromStr` of an enum of words, from one table of
/// variant and word: a variant missing from (or extra in) the table does not compile. The words are
/// the ones of the plan's table; `FromStr` accepts exactly them and nothing else, and an unknown word
/// is an error, never a default.
macro_rules! words {
    ($name:ident, $what:literal, [$($variant:ident => $word:literal),+ $(,)?]) => {
        impl $name {
            /// Every value, in declaration order.
            pub const ALL: &'static [$name] = &[$($name::$variant),+];

            /// The word of this value: the one spelling `FromStr` accepts.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $($name::$variant => $word),+
                }
            }
        }

        impl ::core::fmt::Display for $name {
            fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
                f.write_str(self.as_str())
            }
        }

        /// Strict: exactly one of the words.
        impl ::core::str::FromStr for $name {
            type Err = ::azlin_seams::UnknownWord;

            fn from_str(text: &str) -> Result<$name, ::azlin_seams::UnknownWord> {
                match text {
                    $($word => Ok($name::$variant),)+
                    _ => Err(::azlin_seams::UnknownWord { vocabulary: $what }),
                }
            }
        }
    };
}

pub mod audience;
pub mod bounds;
pub mod id;
pub mod kind;
pub mod options;
pub mod record;
pub mod strength;
pub mod update;
pub mod validate;

// The modules of the second lane (W1-S16c2): each is a stub file that its lane replaces (R2: nobody
// edits this file after it is written).
pub mod counters;
pub mod credential;
pub mod misses;
pub mod notify;
pub mod open_log;
pub mod resolve;

pub use audience::{Audience, MAX_GROUPS, MAX_PEOPLE};
pub use bounds::{apply_policy, Bounded, PolicyViolation};
pub use id::{
    has_link_id_shape, new_id, PublicLinkId, Slug, SlugError, ID_BITS, ID_CHARS, SLUG_MAX_LEN,
    SLUG_MIN_LEN,
};
pub use kind::LinkKind;
pub use options::{
    LinkOptions, Notify, Rights, Sealed, SealedError, MAX_EXPIRES_AFTER_OPEN, MAX_IDLE_EXPIRY,
    MESSAGE_MAX_LEN, SEALED_MAX_LEN, SEALED_MIN_LEN, TITLE_MAX_LEN,
};
pub use record::{
    ByteRange, Counters, DeviceToken, Grant, GrantError, Grants, LinkEnded, LinkRecord, LinkRow,
    LogId, NewLink, ObjectRef, OpaqueRecord, OpaqueRecordError, PausedBy, Pauses, RecordError,
    RequestId, SeenDevice, Wrap, DEVICE_SEEN_DOMAIN, MAX_GRANTS, MAX_WRAPS, OPAQUE_RECORD_MAX_LEN,
    WRAP_MAX_LEN,
};
pub use strength::{Group, OptionName, Strength, STRENGTH};
pub use update::{
    pause, repoint, revoke, rotate, unpause, update, Actor, Change, KeyBound, Rotated,
    RotationReason, UpdateRefusal,
};
pub use validate::{validate, Facts, OptionError, Validated};

/// The mutation guards of lane S16c1 (`azlin_seams::mutant!`), each with the test that must fail
/// when the guard is on: the hold that proves the tests can fail runs
/// `AZLIN_MUTANT=<id> cargo test -p azlin-links --features azlin-seams/mutants` once for each id
/// (COMMON-W1R R19). A guard is listed here when it is in the source of the S16c1 files
/// (`tests/hygiene_tests.rs` reads both). The guards of the resolver are lane S16c2's.
///
/// | id | what the guard turns off | killer |
/// |---|---|---|
/// | `links_slug_without_password_or_name` | "a slug needs a password or a named audience" | `slug_needs_a_password_or_a_named_audience` |
/// | `links_edit_for_unnamed_audience` | "edit needs a named audience" | `edit_needs_a_named_audience` |
/// | `links_comment_for_anonymous` | "comments need a signed-in audience" | `comment_needs_a_signed_in_audience` |
/// | `links_open_log_for_anonymous` | "an open log needs a signed-in audience" | `open_log_needs_a_signed_in_audience_and_is_off_by_default` |
/// | `links_upload_without_paired_request` | "uploads need the paired request" | `allow_upload_needs_the_paired_request` |
/// | `links_people_wraps_not_counted` | "people need one wrap each" | `people_needs_one_wrap_per_person` |
/// | `links_policy_audience_unchecked` | the policy's allowed audiences | `policy_refuses_each_audience_it_does_not_allow` |
/// | `links_policy_anyone_without_password` | the policy's "password for anyone" | `policy_requires_a_password_for_anyone` |
/// | `links_policy_max_expiry_unchecked` | the policy's longest life | `policy_max_expiry` |
/// | `links_policy_forcing_preview_only_skipped` | the forced `preview_only` | `policy_forces_preview_only_watermark_and_app_only` |
/// | `links_policy_forcing_watermark_skipped` | the forced watermark | `policy_forces_preview_only_watermark_and_app_only` |
/// | `links_policy_forcing_app_only_skipped` | the forced `app_only` | `policy_forces_preview_only_watermark_and_app_only` |
/// | `links_slug_may_look_like_an_id` | "a slug never has the shape of an id" | `slug_shape_and_never_a_link_id_shape` |
/// | `links_update_by_anyone` | "only the creator or a manager changes a link" | `only_the_creator_or_a_manager_changes_a_link` |
/// | `links_audience_change_without_rotation` | the rotation for an audience across key strength | `changing_audience_across_key_strength_requires_rotation` |
/// | `links_wrap_removed_without_rotation` | the rotation for a wrap removed | `removing_a_person_requires_rotation` |
/// | `links_password_change_without_rotation` | the rotation for a new password | `password_change_requires_rotation` |
/// | `links_operator_pause_lifted_by_owner` | "only the operator lifts the operator's pause" | `operator_pause_is_lifted_only_by_the_operator` |
/// | `links_revoked_link_changes` | "a revoked link never changes" | `revoke_is_final` |
/// | `links_rotate_keeps_old_id` | "a new id, the old URL stops at once" | `rotation_gives_a_new_id_and_the_old_id_stops_at_once` |
/// | `links_repoint_by_non_writer` | "writers re-point" | `repoint_only_for_folder_live_by_a_writer` |
/// | `links_repoint_any_kind` | "only a live folder is re-pointed" | `repoint_only_for_folder_live_by_a_writer` |
/// | `links_wraps_may_name_strangers` | "the wraps of a people link are the named people" | `the_wraps_of_a_people_link_are_the_named_people` |
/// | `links_frozen_link_changes` | "a link the operator or the reports paused is frozen for its owner" | `a_link_the_operator_or_the_reports_paused_is_frozen_for_its_owner` |
/// | `links_rotate_reuses_request` | "a rotation brings a new paired request" | `a_rotation_needs_a_new_paired_request` |
///
/// Implements planning/architecture.md section 12 (E5) and section 1.4 rule 1 (the guard reads its
/// switch as a parameter of the process).
///
/// Seam: the mutant hold of lane W1-S16c1; S47d (the escalation lane attacks the same rules).
pub const MUTANT_GUARDS: [(&str, &str); 25] = [
    (
        "links_slug_without_password_or_name",
        "slug_needs_a_password_or_a_named_audience",
    ),
    (
        "links_edit_for_unnamed_audience",
        "edit_needs_a_named_audience",
    ),
    (
        "links_comment_for_anonymous",
        "comment_needs_a_signed_in_audience",
    ),
    (
        "links_open_log_for_anonymous",
        "open_log_needs_a_signed_in_audience_and_is_off_by_default",
    ),
    (
        "links_upload_without_paired_request",
        "allow_upload_needs_the_paired_request",
    ),
    (
        "links_people_wraps_not_counted",
        "people_needs_one_wrap_per_person",
    ),
    (
        "links_policy_audience_unchecked",
        "policy_refuses_each_audience_it_does_not_allow",
    ),
    (
        "links_policy_anyone_without_password",
        "policy_requires_a_password_for_anyone",
    ),
    ("links_policy_max_expiry_unchecked", "policy_max_expiry"),
    (
        "links_policy_forcing_preview_only_skipped",
        "policy_forces_preview_only_watermark_and_app_only",
    ),
    (
        "links_policy_forcing_watermark_skipped",
        "policy_forces_preview_only_watermark_and_app_only",
    ),
    (
        "links_policy_forcing_app_only_skipped",
        "policy_forces_preview_only_watermark_and_app_only",
    ),
    (
        "links_slug_may_look_like_an_id",
        "slug_shape_and_never_a_link_id_shape",
    ),
    (
        "links_update_by_anyone",
        "only_the_creator_or_a_manager_changes_a_link",
    ),
    (
        "links_audience_change_without_rotation",
        "changing_audience_across_key_strength_requires_rotation",
    ),
    (
        "links_wrap_removed_without_rotation",
        "removing_a_person_requires_rotation",
    ),
    (
        "links_password_change_without_rotation",
        "password_change_requires_rotation",
    ),
    (
        "links_operator_pause_lifted_by_owner",
        "operator_pause_is_lifted_only_by_the_operator",
    ),
    ("links_revoked_link_changes", "revoke_is_final"),
    (
        "links_rotate_keeps_old_id",
        "rotation_gives_a_new_id_and_the_old_id_stops_at_once",
    ),
    (
        "links_repoint_by_non_writer",
        "repoint_only_for_folder_live_by_a_writer",
    ),
    (
        "links_repoint_any_kind",
        "repoint_only_for_folder_live_by_a_writer",
    ),
    (
        "links_wraps_may_name_strangers",
        "the_wraps_of_a_people_link_are_the_named_people",
    ),
    (
        "links_frozen_link_changes",
        "a_link_the_operator_or_the_reports_paused_is_frozen_for_its_owner",
    ),
    (
        "links_rotate_reuses_request",
        "a_rotation_needs_a_new_paired_request",
    ),
];
