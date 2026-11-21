//! The options of a link: every row of the table of planning/architecture.md section 5.12 as typed
//! data ("Every option below is part of v1").
//!
//! [`LinkOptions`] is plain data with public fields: it is what a client asks for. It enters a link's
//! row only through `validate` (the rules between options) and `apply_policy` (the account's
//! `link_policy`), which hand out witnesses (`Validated`, `Bounded`) that `LinkRecord::create`
//! takes. There is NO `Default`: an audience is a choice, and a lenient default audience would be
//! `anyone`. [`LinkOptions::new`] takes the audience and the kind and turns every other option off
//! ("`open_log`: off by default"; the plan gives no other default: drift register of lane
//! W1-S16c1).
//!
//! `title` and `message` are [`Sealed`]: bytes sealed under the link key by the creating device, so
//! `reg_link` "holds no key, no file name and no title in clear". A node cannot tell sealed bytes
//! from other bytes; the type keeps a plain `String` out of the API and the bounds keep a sealed
//! value small (it holds at least a nonce and a tag, and no more than a cap).
//!
//! Implements planning/architecture.md section 5.12 (the options table; `title`, `message`).

use core::fmt;

use azlin_seams::{ByteCount, Span, Stamp};

use crate::audience::Audience;
use crate::id::Slug;
use crate::kind::LinkKind;
use crate::strength::{OptionName, Strength};

/// The shortest sealed value: a 12-byte nonce and a 16-byte tag, the least any authenticated
/// encryption of the seal produces. Anything shorter cannot be sealed bytes.
///
/// Implements planning/architecture.md section 5.12 (`title`, `message`: "sealed under the link
/// key").
///
/// Caller: `Sealed::new`.
pub const SEALED_MIN_LEN: usize = 28;

/// The longest sealed value of any field: a bound on what a row can hold (1.4 rule 4). The fields
/// have caps of their own below it.
///
/// Implements planning/architecture.md section 1.4 (rule 4, limits).
///
/// Caller: `Sealed::new`.
pub const SEALED_MAX_LEN: usize = 16 * 1024;

/// The longest sealed `title`. An estimate (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 5.12 (`title`) and section 1.4 (rule 4).
///
/// Caller: `validate` (`TitleTooLong`).
pub const TITLE_MAX_LEN: usize = 512;

/// The longest sealed `message`. An estimate (drift register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 5.12 (`message`) and section 1.4 (rule 4).
///
/// Caller: `validate` (`MessageTooLong`).
pub const MESSAGE_MAX_LEN: usize = 4096;

/// The longest `expires_after_open`: ten years. The plan gives no bound; an unbounded duration would
/// make the sum with `first_open_at` a question of overflow for the resolver. An estimate (drift
/// register of lane W1-S16c1).
///
/// Implements planning/architecture.md section 5.12 (`expires_after_open`) and section 1.4 (rule 4).
///
/// Caller: `validate` (`TooLong`).
pub const MAX_EXPIRES_AFTER_OPEN: Span = Span::days(3650);

/// The longest `idle_expiry`: ten years (see [`MAX_EXPIRES_AFTER_OPEN`]).
///
/// Implements planning/architecture.md section 5.12 (`idle_expiry`) and section 1.4 (rule 4).
///
/// Caller: `validate` (`TooLong`).
pub const MAX_IDLE_EXPIRY: Span = Span::days(3650);

/// `download` or `preview_only` (the `rights` choice): preview only means no save, export, print or
/// copy, screenshots blocked where the platform allows. Strength: app.
///
/// Implements planning/architecture.md section 5.12 (`download` or `preview_only`).
///
/// Caller: `LinkOptions::rights`, `apply_policy` (`force_preview_only`); Seam: the apps (they obey
/// it).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Rights {
    /// The recipient may save the files.
    Download,
    /// The recipient may look: no save, export, print or copy (the apps obey it).
    PreviewOnly,
}

words!(
    Rights,
    "link rights",
    [Download => "download", PreviewOnly => "preview_only"]
);

/// `notify`: when the sharer is told about an open, delivered as `link_used` (5.13).
///
/// Implements planning/architecture.md section 5.12 (`notify`: never, the first open, every open).
///
/// Caller: `LinkOptions::notify`; Seam: S16c2 (`count_open` says whether to tell), S33 (`link_used`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Notify {
    /// Never.
    Never,
    /// At the first open.
    FirstOpen,
    /// At every open.
    EveryOpen,
}

words!(
    Notify,
    "link notify",
    [Never => "never", FirstOpen => "first_open", EveryOpen => "every_open"]
);

impl Notify {
    /// How the plan's cell writes the value: "never, the first open, every open".
    ///
    /// Implements planning/architecture.md section 5.12 (`notify`).
    pub const fn doc_phrase(self) -> &'static str {
        match self {
            Notify::Never => "never",
            Notify::FirstOpen => "the first open",
            Notify::EveryOpen => "every open",
        }
    }
}

/// Why bytes are no sealed value.
///
/// Implements planning/architecture.md section 5.12 (`title`, `message`: sealed).
///
/// Caller: `Sealed::new`, `Wrap::new`; Seam: the share dialog (it shows the reason).
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum SealedError {
    /// Fewer than [`SEALED_MIN_LEN`] bytes: no nonce and tag fit.
    #[error(
        "a sealed value is at least {} bytes (a nonce and a tag), not {len}",
        SEALED_MIN_LEN
    )]
    TooShort {
        /// The length given.
        len: usize,
    },
    /// More than the cap of the value.
    #[error("a sealed value is too long: {len} bytes")]
    TooLong {
        /// The length given.
        len: usize,
    },
}

/// Bytes sealed under the link key (or under a member key, for a wrap): opaque to this crate, at
/// least [`SEALED_MIN_LEN`] and at most [`SEALED_MAX_LEN`] bytes. It prints as its length only.
///
/// Implements planning/architecture.md section 5.12 (`title`, `message`: "sealed under the link key,
/// so only recipients read them"; "`reg_link` holds no ... title in clear").
///
/// Caller: `LinkOptions::title`, `LinkOptions::message`, `Wrap`; Seam: S15b (the envelope that
/// produces the bytes), S16d (the share dialog seals the title).
#[derive(Clone, PartialEq, Eq)]
pub struct Sealed(Vec<u8>);

impl Sealed {
    /// The sealed value with these bytes; an error when they are too short or too long.
    pub fn new(bytes: Vec<u8>) -> Result<Sealed, SealedError> {
        let len = bytes.len();
        if len < SEALED_MIN_LEN {
            return Err(SealedError::TooShort { len });
        }
        if len > SEALED_MAX_LEN {
            return Err(SealedError::TooLong { len });
        }
        Ok(Sealed(bytes))
    }

    /// The sealed bytes.
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The number of bytes.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Whether there are no bytes (never: the least length is [`SEALED_MIN_LEN`]).
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl fmt::Debug for Sealed {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Sealed({} bytes)", self.0.len())
    }
}

/// Every option of a link: one field for each row of the table of planning/architecture.md section
/// 5.12 (`download` or `preview_only` is the one field `rights`).
///
/// Implements planning/architecture.md section 5.12 (the options table).
///
/// Caller: `validate`, `apply_policy`, `LinkRecord`, `update`; Seam: S16c2 (`resolve` reads every
/// server option), S16d (the share dialog builds one), LINKS-NODE (`link_create`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct LinkOptions {
    /// `audience`: who may open the link.
    pub audience: Audience,
    /// `password`: with any audience; checked with OPAQUE (the record is kept in the row).
    pub password: bool,
    /// `app_only`: the link does not open in `drive.azlin.io`.
    pub app_only: bool,
    /// `kind`: what the link shares.
    pub kind: LinkKind,
    /// `subfolders`: with or without (the folder kinds only).
    pub subfolders: bool,
    /// `not_before`: the link opens from this date.
    pub not_before: Option<Stamp>,
    /// `expires_at`: the link ends at this date.
    pub expires_at: Option<Stamp>,
    /// `expires_after_open`: a duration from the first open.
    pub expires_after_open: Option<Span>,
    /// `idle_expiry`: a duration without an open (the plan counts days).
    pub idle_expiry: Option<Span>,
    /// `one_time`: the first complete download revokes it.
    pub one_time: bool,
    /// `max_downloads`: a count of complete downloads.
    pub max_downloads: Option<u32>,
    /// `bytes_cap`: a number of bytes served.
    pub bytes_cap: Option<ByteCount>,
    /// `max_devices`: distinct devices or browsers, counted by a random token each keeps.
    pub max_devices: Option<u16>,
    /// `download` or `preview_only`.
    pub rights: Rights,
    /// `allow_upload`: recipients add files through the paired request.
    pub allow_upload: bool,
    /// `edit`: the recipient becomes an external member with write access.
    pub edit: bool,
    /// `comment`: recipients append to the link's comment log.
    pub comment: bool,
    /// `allow_reshare`: whether the apps offer to share it on.
    pub allow_reshare: bool,
    /// `watermark`: previews carry the viewer's account name.
    pub watermark: bool,
    /// `notify`: never, the first open, every open.
    pub notify: Notify,
    /// `open_log`: which account opened it and when (off by default).
    pub open_log: bool,
    /// `title`: sealed under the link key.
    pub title: Option<Sealed>,
    /// `message`: sealed under the link key.
    pub message: Option<Sealed>,
    /// `slug`: a readable name instead of the random id.
    pub slug: Option<Slug>,
}

impl LinkOptions {
    /// The options of a link for `audience` over `kind`, every other option off: no password, no
    /// dates, no caps, `download`, no upload, edit or comment, no reshare, no watermark, never tell
    /// the sharer, no open log, no title, no message, no slug.
    ///
    /// Implements planning/architecture.md section 5.12 (`open_log`: "off by default").
    ///
    /// Caller: the tests of this crate; Seam: S16d (the share dialog starts from it), LINKS-NODE.
    pub fn new(audience: Audience, kind: LinkKind) -> LinkOptions {
        LinkOptions {
            audience,
            password: false,
            app_only: false,
            kind,
            subfolders: false,
            not_before: None,
            expires_at: None,
            expires_after_open: None,
            idle_expiry: None,
            one_time: false,
            max_downloads: None,
            bytes_cap: None,
            max_devices: None,
            rights: Rights::Download,
            allow_upload: false,
            edit: false,
            comment: false,
            allow_reshare: false,
            watermark: false,
            notify: Notify::Never,
            open_log: false,
            title: None,
            message: None,
            slug: None,
        }
    }

    /// The strengths the apps show next to `name` for THESE options: the table's strengths, except
    /// `audience`, whose strength is the one of the chosen audience.
    ///
    /// Implements planning/architecture.md section 5.12 ("the apps show it next to the option").
    ///
    /// Seam: the share dialog of AzDrive and AzWorkspace (the label next to each option).
    pub fn strengths(&self, name: OptionName) -> &'static [Strength] {
        match name {
            OptionName::Audience => match self.audience.strength() {
                Strength::Key => &[Strength::Key],
                Strength::Server => &[Strength::Server],
                Strength::App => &[Strength::App],
            },
            other => other.strengths(),
        }
    }
}
