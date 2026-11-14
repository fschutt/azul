//! The options of a link (planning/architecture.md 5.12 "Sharing: links, requests, external members", the
//! table every option of which "is part of v1"): the groups, the options, the values of the enumerated ones
//! and the STRENGTH of each, which the apps show next to the option.
//!
//! The strengths are the plan's three: **key** (enforced by cryptography: it holds even if our servers are
//! broken into), **server** (the nodes refuse to serve the ciphertext: it holds for anyone who has not
//! downloaded it already) and **app** (the Azlin apps obey it, a modified app would not). azlin-proto's doc
//! tests parse the plan's table and fail when a row, a group, a value or a strength differs from the
//! constants here. The record a link is stored as (`reg_link`) and the typed options of a link are the sync
//! and sharing unit's (S16); what they must agree with is this table.

text_enum! {
    /// The groups of the plan's table, in its order.
    pub enum LinkGroup {
        Who = "who",
        What = "what",
        When = "when",
        HowMuch = "how much",
        Rights = "rights",
        TellingTheSharer = "telling the sharer",
        Presentation = "presentation",
    }
}

text_enum! {
    /// How an option is enforced (5.12).
    pub enum Strength {
        /// Enforced by cryptography; it holds even if our servers are broken into.
        Key = "key",
        /// The nodes refuse to serve the ciphertext; it holds for anyone who has not downloaded it already.
        Server = "server",
        /// The Azlin apps obey it; a modified app would not.
        App = "app",
    }
}

text_enum! {
    /// `audience`: who may open the link.
    pub enum LinkAudience {
        /// Anyone with the link.
        Anyone = "anyone",
        /// Any Azlin account.
        SignedIn = "signed_in",
        /// This account, or chosen groups.
        Members = "members",
        /// Named people: the share key is wrapped to each one's member key, so a forwarded URL opens nothing.
        People = "people",
        /// Anyone may ask, and a device of the sharer approves.
        Approval = "approval",
    }
}

impl LinkAudience {
    /// The strength of the audience (the plan's cell: "key for `anyone`, `people` and `approval`; server
    /// for `signed_in` and `members`").
    #[must_use]
    pub const fn strength(self) -> Strength {
        match self {
            LinkAudience::Anyone | LinkAudience::People | LinkAudience::Approval => Strength::Key,
            LinkAudience::SignedIn | LinkAudience::Members => Strength::Server,
        }
    }
}

text_enum! {
    /// `kind`: what the link shares.
    pub enum LinkKind {
        File = "file",
        /// Hand-picked files.
        Selection = "selection",
        FolderSnapshot = "folder_snapshot",
        /// Follows edits.
        FolderLive = "folder_live",
        /// One pinned version.
        Version = "version",
    }
}

text_enum! {
    /// `download` or `preview_only` (rights): preview only means no save, export, print or copy.
    pub enum LinkRights {
        Download = "download",
        PreviewOnly = "preview_only",
    }
}

text_enum! {
    /// `notify`: telling the sharer, delivered as `link_used` (5.13).
    pub enum LinkNotify {
        Never = "never",
        FirstOpen = "first_open",
        EveryOpen = "every_open",
    }
}

impl LinkNotify {
    /// How the plan's cell writes the value ("never, the first open, every open").
    #[must_use]
    pub const fn doc_phrase(self) -> &'static str {
        match self {
            LinkNotify::Never => "never",
            LinkNotify::FirstOpen => "the first open",
            LinkNotify::EveryOpen => "every open",
        }
    }
}

/// One row of the plan's table: the group, the option or options the row names (backticked in its second
/// column, in order) and how the row is enforced.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LinkOptionRow {
    pub group: LinkGroup,
    pub options: &'static [&'static str],
    /// The names are the two values of ONE choice (the plan writes "`download` or `preview_only`").
    pub choice: bool,
    /// The strengths the row's cell names. For `audience` it is both: [`LinkAudience::strength`] says which
    /// audience has which.
    pub strength: &'static [Strength],
}

const fn row(
    group: LinkGroup,
    options: &'static [&'static str],
    strength: &'static [Strength],
) -> LinkOptionRow {
    LinkOptionRow {
        group,
        options,
        choice: false,
        strength,
    }
}

use LinkGroup::{HowMuch, Presentation, Rights, TellingTheSharer, What, When, Who};
use Strength::{App, Key, Server};

/// The plan's table of link options, row for row (a row that continues its group repeats the group here).
pub const LINK_OPTIONS: &[LinkOptionRow] = &[
    row(Who, &["audience"], &[Key, Server]),
    row(Who, &["password"], &[Key, Server]),
    row(Who, &["app_only"], &[App]),
    row(What, &["kind"], &[Key]),
    row(What, &["subfolders"], &[Key]),
    row(When, &["not_before", "expires_at"], &[Server]),
    row(When, &["expires_after_open"], &[Server]),
    row(When, &["idle_expiry"], &[Server]),
    row(When, &["one_time"], &[Server]),
    row(HowMuch, &["max_downloads", "bytes_cap"], &[Server]),
    row(HowMuch, &["max_devices"], &[Server]),
    LinkOptionRow {
        group: Rights,
        options: &["download", "preview_only"],
        choice: true,
        strength: &[App],
    },
    row(Rights, &["allow_upload"], &[Key, Server]),
    row(Rights, &["edit"], &[Key, Server]),
    row(Rights, &["comment"], &[Server]),
    row(Rights, &["allow_reshare"], &[App]),
    row(Rights, &["watermark"], &[App]),
    row(TellingTheSharer, &["notify"], &[Server]),
    row(TellingTheSharer, &["open_log"], &[Server]),
    row(Presentation, &["title", "message"], &[Key]),
    row(Presentation, &["slug"], &[Server]),
];
