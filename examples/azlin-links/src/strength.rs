//! How a link option is enforced, and the table of it (planning/architecture.md section 5.12):
//! "Each has one of three strengths, and the apps show it next to the option".
//!
//! - **key**: enforced by cryptography; it holds even if our servers are broken into;
//! - **server**: the nodes refuse to serve the ciphertext; it holds for anyone who has not
//!   downloaded it already;
//! - **app**: the Azlin apps obey it, a modified app would not.
//!
//! [`OptionName`] names the 24 options of the table (the same names, in the same order, as
//! `azlin_policy::LINK_OPTION_NAMES`: the choice "`download` or `preview_only`" is ONE option, which
//! carries the name of its group, `rights`), [`Group`] its seven groups and [`STRENGTH`] the strength
//! cell of every row. The doc-table test of this crate (`option_strengths_match_architecture_5_12_table`)
//! parses the plan's table and compares it with [`STRENGTH`]; a second test compares it with the
//! contract table of S5 (`azlin-contract`, `fixtures/c4/link_options.json`).
//!
//! Implements planning/architecture.md section 5.12 (the options table).

/// How an option is enforced (planning/architecture.md section 5.12).
///
/// Implements planning/architecture.md section 5.12 ("Each has one of three strengths").
///
/// Caller: `STRENGTH`, `Audience::strength`; Seam: the share dialog of AzDrive (it shows the strength
/// next to the option, "Preview only (our apps won't save it)"), S5's doc-table tests.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Strength {
    /// Enforced by cryptography: it holds even if our servers are broken into.
    Key,
    /// The nodes refuse to serve the ciphertext: it holds for anyone who has not downloaded it
    /// already.
    Server,
    /// The Azlin apps obey it; a modified app would not.
    App,
}

words!(
    Strength,
    "link strength",
    [Key => "key", Server => "server", App => "app"]
);

/// The seven groups of the table of planning/architecture.md section 5.12, in its order.
///
/// Implements planning/architecture.md section 5.12 (the first column of the options table).
///
/// Caller: `OptionName::group`; Seam: the share dialog (it lays the options out by group).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Group {
    /// `audience`, `password`, `app_only`.
    Who,
    /// `kind`, `subfolders`.
    What,
    /// `not_before`, `expires_at`, `expires_after_open`, `idle_expiry`, `one_time`.
    When,
    /// `max_downloads`, `bytes_cap`, `max_devices`.
    HowMuch,
    /// `rights`, `allow_upload`, `edit`, `comment`, `allow_reshare`, `watermark`.
    Rights,
    /// `notify`, `open_log`.
    TellingTheSharer,
    /// `title`, `message`, `slug`.
    Presentation,
}

words!(
    Group,
    "link option group",
    [
        Who => "who",
        What => "what",
        When => "when",
        HowMuch => "how much",
        Rights => "rights",
        TellingTheSharer => "telling the sharer",
        Presentation => "presentation",
    ]
);

/// One option of a link: one row of the table of planning/architecture.md section 5.12, with the
/// choice "`download` or `preview_only`" as ONE option, `rights`. 24 names, in the order of the table.
///
/// Implements planning/architecture.md section 5.12 ("Every option below is part of v1").
///
/// Caller: `STRENGTH`, `OptionError::option`, `PolicyViolation::options`, `Change::touches`;
/// Seam: the share dialog (an error names the option it is about).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum OptionName {
    /// `audience`: who may open the link.
    Audience,
    /// `password`: with any audience; checked with OPAQUE.
    Password,
    /// `app_only`: the link does not open in `drive.azlin.io`.
    AppOnly,
    /// `kind`: what the link shares.
    Kind,
    /// `subfolders`: with or without.
    Subfolders,
    /// `not_before`: the link opens from this date.
    NotBefore,
    /// `expires_at`: the link ends at this date.
    ExpiresAt,
    /// `expires_after_open`: a duration from the first open.
    ExpiresAfterOpen,
    /// `idle_expiry`: days without an open.
    IdleExpiry,
    /// `one_time`: the first complete download revokes it.
    OneTime,
    /// `max_downloads`: a count.
    MaxDownloads,
    /// `bytes_cap`: a number of bytes.
    BytesCap,
    /// `max_devices`: distinct devices or browsers.
    MaxDevices,
    /// `rights`: `download` or `preview_only`.
    Rights,
    /// `allow_upload`: recipients add files through a paired request.
    AllowUpload,
    /// `edit`: the recipient becomes an external member with write access.
    Edit,
    /// `comment`: recipients append to the link's comment log.
    Comment,
    /// `allow_reshare`: whether the apps offer to share it on.
    AllowReshare,
    /// `watermark`: previews carry the viewer's name.
    Watermark,
    /// `notify`: never, the first open, every open.
    Notify,
    /// `open_log`: which account opened it and when.
    OpenLog,
    /// `title`: sealed under the link key.
    Title,
    /// `message`: sealed under the link key.
    Message,
    /// `slug`: a readable name instead of the random id.
    Slug,
}

words!(
    OptionName,
    "link option",
    [
        Audience => "audience",
        Password => "password",
        AppOnly => "app_only",
        Kind => "kind",
        Subfolders => "subfolders",
        NotBefore => "not_before",
        ExpiresAt => "expires_at",
        ExpiresAfterOpen => "expires_after_open",
        IdleExpiry => "idle_expiry",
        OneTime => "one_time",
        MaxDownloads => "max_downloads",
        BytesCap => "bytes_cap",
        MaxDevices => "max_devices",
        Rights => "rights",
        AllowUpload => "allow_upload",
        Edit => "edit",
        Comment => "comment",
        AllowReshare => "allow_reshare",
        Watermark => "watermark",
        Notify => "notify",
        OpenLog => "open_log",
        Title => "title",
        Message => "message",
        Slug => "slug",
    ]
);

impl OptionName {
    /// The group of the table the option is in.
    ///
    /// Implements planning/architecture.md section 5.12 (the first column of the options table).
    pub const fn group(self) -> Group {
        match self {
            OptionName::Audience | OptionName::Password | OptionName::AppOnly => Group::Who,
            OptionName::Kind | OptionName::Subfolders => Group::What,
            OptionName::NotBefore
            | OptionName::ExpiresAt
            | OptionName::ExpiresAfterOpen
            | OptionName::IdleExpiry
            | OptionName::OneTime => Group::When,
            OptionName::MaxDownloads | OptionName::BytesCap | OptionName::MaxDevices => {
                Group::HowMuch
            }
            OptionName::Rights
            | OptionName::AllowUpload
            | OptionName::Edit
            | OptionName::Comment
            | OptionName::AllowReshare
            | OptionName::Watermark => Group::Rights,
            OptionName::Notify | OptionName::OpenLog => Group::TellingTheSharer,
            OptionName::Title | OptionName::Message | OptionName::Slug => Group::Presentation,
        }
    }

    /// The strengths of the option, as the strength cell of the table names them. `audience` has
    /// both: which audience has which is `Audience::strength`.
    ///
    /// Implements planning/architecture.md section 5.12 (the strength column: the ONE place this
    /// crate writes it; [`STRENGTH`] is built from it).
    pub const fn strengths(self) -> &'static [Strength] {
        match self {
            OptionName::Audience | OptionName::Password => &[Strength::Key, Strength::Server],
            OptionName::AppOnly => &[Strength::App],
            OptionName::Kind | OptionName::Subfolders => &[Strength::Key],
            OptionName::NotBefore
            | OptionName::ExpiresAt
            | OptionName::ExpiresAfterOpen
            | OptionName::IdleExpiry
            | OptionName::OneTime
            | OptionName::MaxDownloads
            | OptionName::BytesCap
            | OptionName::MaxDevices => &[Strength::Server],
            OptionName::Rights => &[Strength::App],
            OptionName::AllowUpload | OptionName::Edit => &[Strength::Key, Strength::Server],
            OptionName::Comment => &[Strength::Server],
            OptionName::AllowReshare | OptionName::Watermark => &[Strength::App],
            OptionName::Notify | OptionName::OpenLog => &[Strength::Server],
            OptionName::Title | OptionName::Message => &[Strength::Key],
            OptionName::Slug => &[Strength::Server],
        }
    }
}

/// The strength cell of every row of the options table of planning/architecture.md section 5.12,
/// one entry for each of the 24 options, in the order of the table (the order of
/// `azlin_policy::LINK_OPTION_NAMES`). "The apps show it next to the option."
///
/// Implements planning/architecture.md section 5.12 (the strength column).
///
/// Caller: the doc-table tests of this crate and of S5 (INT-PLAN-TABLES);
/// Seam: the share dialog of AzDrive and AzWorkspace (the label next to each option).
pub const STRENGTH: [(OptionName, &[Strength]); 24] = [
    (OptionName::Audience, OptionName::Audience.strengths()),
    (OptionName::Password, OptionName::Password.strengths()),
    (OptionName::AppOnly, OptionName::AppOnly.strengths()),
    (OptionName::Kind, OptionName::Kind.strengths()),
    (OptionName::Subfolders, OptionName::Subfolders.strengths()),
    (OptionName::NotBefore, OptionName::NotBefore.strengths()),
    (OptionName::ExpiresAt, OptionName::ExpiresAt.strengths()),
    (
        OptionName::ExpiresAfterOpen,
        OptionName::ExpiresAfterOpen.strengths(),
    ),
    (OptionName::IdleExpiry, OptionName::IdleExpiry.strengths()),
    (OptionName::OneTime, OptionName::OneTime.strengths()),
    (
        OptionName::MaxDownloads,
        OptionName::MaxDownloads.strengths(),
    ),
    (OptionName::BytesCap, OptionName::BytesCap.strengths()),
    (OptionName::MaxDevices, OptionName::MaxDevices.strengths()),
    (OptionName::Rights, OptionName::Rights.strengths()),
    (OptionName::AllowUpload, OptionName::AllowUpload.strengths()),
    (OptionName::Edit, OptionName::Edit.strengths()),
    (OptionName::Comment, OptionName::Comment.strengths()),
    (
        OptionName::AllowReshare,
        OptionName::AllowReshare.strengths(),
    ),
    (OptionName::Watermark, OptionName::Watermark.strengths()),
    (OptionName::Notify, OptionName::Notify.strengths()),
    (OptionName::OpenLog, OptionName::OpenLog.strengths()),
    (OptionName::Title, OptionName::Title.strengths()),
    (OptionName::Message, OptionName::Message.strengths()),
    (OptionName::Slug, OptionName::Slug.strengths()),
];
