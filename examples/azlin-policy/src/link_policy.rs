//! The `link_policy` row of the account policy: what the account lets its members do with a link, as
//! typed data. A link has the options of the table of planning/architecture.md section 5.12; the
//! account policy bounds them: it names the audiences that are allowed, the longest expiry, whether
//! a password is required for `anyone`, forces `preview_only`, the watermark and `app_only`, and says
//! whether uploads, edit, comments, open logs and slugs are allowed at all ("`link_policy` (5.10)
//! decides which options members may use and forces some; groups only tighten. A workspace can
//! switch public links off for its members").
//!
//! This crate holds the BOUNDS; `azlin-links` (lane S16c) holds the options themselves and checks a
//! link against the bounds. The five audiences of a link are named here as classes
//! ([`AudienceClass`]): the policy allows a class or not; who is named inside a class (the groups of
//! `members`, the people of `people`) is the link's own business. [`LINK_OPTION_NAMES`] is the list
//! of the option names of the table of section 5.12, which the doc-table tests of this crate and of
//! `azlin-links` both read.
//!
//! Implements planning/architecture.md section 5.10 (row `link_policy`) and section 5.12 (the link
//! options, "Admin bounds").

use core::fmt;

use azlin_seams::Span;

/// The five audiences of a link (`audience` of planning/architecture.md section 5.12), as the
/// classes the policy allows or forbids: `anyone` with the link; `signed_in` (any Azlin account);
/// `members` (this account, or chosen groups); `people` (named people); `approval` (anyone may ask,
/// and a device of the sharer approves).
///
/// Implements planning/architecture.md section 5.12 (`audience`, values `anyone`, `signed_in`,
/// `members`, `people`, `approval`).
///
/// Caller: `AudienceSet`; Seam: S16c1 (`apply_policy` maps each `Audience` of a link to its class).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum AudienceClass {
    /// Anyone with the link (the public link).
    Anyone,
    /// Any Azlin account.
    SignedIn,
    /// This account, or chosen groups of it.
    Members,
    /// Named people: the share key is wrapped to each one's member key.
    People,
    /// Anyone may ask, and a device of the sharer approves.
    Approval,
}

words!(
    AudienceClass,
    "link audience",
    [
        Anyone => "anyone",
        SignedIn => "signed_in",
        Members => "members",
        People => "people",
        Approval => "approval",
    ]
);

impl AudienceClass {
    /// The bit of this class in an [`AudienceSet`].
    const fn bit(self) -> u8 {
        match self {
            AudienceClass::Anyone => 1,
            AudienceClass::SignedIn => 2,
            AudienceClass::Members => 4,
            AudienceClass::People => 8,
            AudienceClass::Approval => 16,
        }
    }
}

/// A set of audience classes: the audiences a policy allows. A set of five bits, so it is `Copy`,
/// and every operation is total. The empty set allows no link at all.
///
/// Implements planning/architecture.md section 5.10 (`link_policy`: "the audiences allowed") and
/// section 5.12 ("A workspace can switch public links off for its members": the set without
/// `anyone`).
///
/// Caller: `LinkPolicy`, `Policy::defaults`, `check_tightens` (subset order), `clamp`
/// (intersection); Seam: S16c1 (`apply_policy`: `contains`).
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct AudienceSet(u8);

impl AudienceSet {
    /// The set of no audience: no link of any audience is allowed.
    pub const NONE: AudienceSet = AudienceSet(0);

    /// The set of every audience: the loosest bound.
    pub const ALL: AudienceSet = AudienceSet(0b1_1111);

    /// Whether `class` is in the set.
    pub const fn contains(self, class: AudienceClass) -> bool {
        self.0 & class.bit() != 0
    }

    /// The set with `class` added.
    #[must_use]
    pub const fn with(self, class: AudienceClass) -> AudienceSet {
        AudienceSet(self.0 | class.bit())
    }

    /// The set with `class` taken out.
    #[must_use]
    pub const fn without(self, class: AudienceClass) -> AudienceSet {
        AudienceSet(self.0 & !class.bit())
    }

    /// The classes that are in both sets.
    #[must_use]
    pub const fn intersection(self, other: AudienceSet) -> AudienceSet {
        AudienceSet(self.0 & other.0)
    }

    /// Whether every class of this set is in `other` (the empty set is a subset of every set).
    pub const fn is_subset_of(self, other: AudienceSet) -> bool {
        self.0 & !other.0 == 0
    }

    /// Whether no class is in the set.
    pub const fn is_empty(self) -> bool {
        self.0 == 0
    }

    /// How many classes are in the set.
    pub const fn len(self) -> usize {
        self.0.count_ones() as usize
    }

    /// The classes of the set, in the order of [`AudienceClass::ALL`].
    pub fn classes(self) -> Vec<AudienceClass> {
        AudienceClass::ALL
            .iter()
            .copied()
            .filter(|class| self.contains(*class))
            .collect()
    }
}

/// `anyone,signed_in`: the words of the classes in order, joined by commas; `none` for the empty
/// set.
impl fmt::Display for AudienceSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_empty() {
            return f.write_str("none");
        }
        let mut first = true;
        for class in self.classes() {
            if !first {
                f.write_str(",")?;
            }
            first = false;
            f.write_str(class.as_str())?;
        }
        Ok(())
    }
}

impl fmt::Debug for AudienceSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "AudienceSet({self})")
    }
}

/// The `link_policy` row of planning/architecture.md section 5.10: "the audiences allowed, the
/// maximum expiry, a password required for `anyone`, forced `preview_only`, forced watermark,
/// `app_only` forced, whether uploads, edit, comments, open logs and slugs are allowed (5.12)".
/// One field for each item. The `force_*` fields and `password_for_anyone` are restrictions (`true`
/// is the tighter value); the five allow-flags are permissions (`false` is the tighter value).
///
/// Implements planning/architecture.md section 5.10 (row `link_policy`) and section 5.12 ("Admin
/// bounds": `link_policy` decides which options members may use and forces some; groups only
/// tighten).
///
/// Caller: `Policy`, `check_tightens`, `clamp`; Seam: S16c1 (`azlin-links` `apply_policy`: the
/// bounds of a link).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkPolicy {
    /// The audiences a link may have. Without `anyone`, the account has no public links.
    pub audiences: AudienceSet,
    /// The longest time a link may live (from its creation to `expires_at`); `None`: no bound.
    pub max_expiry: Option<Span>,
    /// A link for `anyone` must have a password.
    pub password_for_anyone: bool,
    /// Every link is `preview_only`: no save, export, print or copy (strength: app).
    pub force_preview_only: bool,
    /// Every preview carries the watermark.
    pub force_watermark: bool,
    /// Every link is `app_only`: it does not open in `drive.azlin.io`.
    pub force_app_only: bool,
    /// Whether a link may take uploads (`allow_upload`).
    pub uploads: bool,
    /// Whether a link may make its recipient an external member with write access (`edit`).
    pub edit: bool,
    /// Whether a link may take comments (`comment`).
    pub comments: bool,
    /// Whether a link may keep an open log (`open_log`).
    pub open_logs: bool,
    /// Whether a link may have a readable name instead of its random id (`slug`).
    pub slugs: bool,
}

impl LinkPolicy {
    /// The policy that bounds nothing: every audience, no longest expiry, no forced option and every
    /// allow-flag on. Planning/architecture.md section 5.10 gives no `link_policy` value for any
    /// kind, so this is the default of every kind (drift register of lane W1-S17b).
    ///
    /// Implements planning/architecture.md section 5.10 (row `link_policy`).
    ///
    /// Caller: `Policy::defaults`.
    pub const fn allow_all() -> LinkPolicy {
        LinkPolicy {
            audiences: AudienceSet::ALL,
            max_expiry: None,
            password_for_anyone: false,
            force_preview_only: false,
            force_watermark: false,
            force_app_only: false,
            uploads: true,
            edit: true,
            comments: true,
            open_logs: true,
            slugs: true,
        }
    }
}

/// The option names of the table of planning/architecture.md section 5.12, in the order of the
/// table, one name per option: the two names of a choice (`download` or `preview_only`) are the two
/// values of ONE option, which carries the name of its group, `rights`; every other name is the
/// backticked name of the table (24 in all, from 25 backticked names). The doc-table test of this
/// crate (`link_options_table_of_5_12_parses`) fails when the document adds or drops an option, and
/// `azlin-links` reads the same list.
///
/// Implements planning/architecture.md section 5.12 ("Every option below is part of v1").
///
/// Seam: S16c1 (`azlin-links` test `option_names_equal_azlin_policys_link_option_names`).
pub const LINK_OPTION_NAMES: [&str; 24] = [
    "audience",
    "password",
    "app_only",
    "kind",
    "subfolders",
    "not_before",
    "expires_at",
    "expires_after_open",
    "idle_expiry",
    "one_time",
    "max_downloads",
    "bytes_cap",
    "max_devices",
    "rights",
    "allow_upload",
    "edit",
    "comment",
    "allow_reshare",
    "watermark",
    "notify",
    "open_log",
    "title",
    "message",
    "slug",
];
