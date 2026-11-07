//! The blocking storage work of AzDrive, one [`Job`] per azul `Thread`: the
//! open folder's scan (its batches streamed while it reads), the stat of the
//! rows in view, the item counts of folders, a tree node's folders, a
//! transfer plan, a transfer run (with progress messages while it copies), a
//! delete into the trash or for good, a rename, a new folder or file, an
//! undo, a preview, a folder's size, an object's metadata, a zip, the
//! settings file, a download to open; and the Add drive dialog's calls (a
//! source's connection test, Azlin's storage tiers, a test drive, a checkout
//! and the wait for its payment) with the claims of the checkouts no dialog
//! waits for (in the background, at every start). Every answer comes back to the UI
//! thread as an [`Outcome`] through the thread's write-back. No callback
//! ever waits on a drive.

use std::{
    collections::{HashMap, HashSet},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, AtomicUsize, Ordering},
        Arc,
    },
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use azcloud_kit::{
    pending::{self, Abandoned, Claimed, Final, Finished, PendingTokens, Polled, Recorded},
    change_due, look_at_drive, reminder_due, ActivationCode, Checkout, CheckoutFor, CheckoutVia,
    ClaimKey, CloudError, DriveBundle, DriveTokens, GrantChanged, OptionsQuery, PendingCheckout,
    PeriodTokenStore, PeriodTokens, Quote, SharedKeyring, TierChanged, Tiers, TokenError,
    TokenServer, UserError, VoucherRedeemed,
};
use azul_appkit::l10n::{Phrase, Text};
use azul_pay::{CashSlip, Choice, Created, Look, SurfaceKind};
use azul::{
    image::{ImageRef, RawImage},
    prelude::*,
    vec::U8VecRef,
};
use azul_search_index::{
    DriveIndex, ExtractFn, Extractors, IndexStatus, Kind, UpdateProgress, UpdateSummary,
};
use azul_storage::{
    azul_transport::AzulTransport,
    beneath::Dir,
    config::DriveEntry,
    ctl::Quiet,
    durable::Expect,
    ops as storage_ops,
    scratch::{self, MadeState, Scratch},
    transfer, BatchDelete, ByteRange, Drive, DriveError, ListPage, ListRequest, LocalDrive,
    ObjectInfo, Precondition,
};

use crate::{
    browse::{self, Entry},
    fileops::{self, Plan, Progress, SourceItem, TransferKind, TransferReport},
    find::{self, FindEnd, FindPhase},
    l10n::{cloud_error_text, drive_error_text, token_error_text},
    listing::{self, Stat},
    preview::{self, PreviewKind},
    TreeKey, USER_AGENT,
};

/// The keys a bucket's listing asks for per page while a scan streams it.
const SCAN_PAGE: u32 = 1000;

/// The listings a cloud search runs side by side (one subfolder each).
const LIST_WORKERS: usize = 8;

/// What a preview shows, once fetched.
#[derive(Clone)]
pub(crate) enum PreviewContent {
    /// Decoded, ready for `Dom::create_image`.
    Image {
        image: ImageRef,
        width: usize,
        height: usize,
    },
    /// A text's start; `cut`: the file goes on (the pane says so).
    Text { text: String, cut: bool },
    /// A local file the video widget plays.
    Video(PathBuf),
    /// A WAV file's samples, for azul's AudioSink.
    Audio(preview::WavSamples),
    /// Why there is nothing to show.
    Message(Text),
}

/// "No preview: " and why.
fn no_preview(why: Text) -> PreviewContent {
    PreviewContent::Message(Text::key("azdrive-preview-failed").then(" ").then(why))
}

/// A test drive a development token server made: its bundle, its session's keyring text, and
/// why the keyring did not take the session (`None`: it is there, written under the drive's
/// lock).
pub(crate) struct BoughtDrive {
    pub bundle: DriveBundle,
    /// Never printed.
    pub session: String,
    pub unsaved: Option<String>,
}

/// How the order button's checkout is paid (azul-pay): the pill's choice, the first surface,
/// the fields page's look, the VAT country (the buyer's declared country), the consent, and the
/// payer's details for a method whose provider wants them from the app (sent with this one
/// checkout, dropped with this value).
pub(crate) struct PayVia {
    pub choice: Choice,
    pub surface: SurfaceKind,
    pub look: Look,
    pub country: String,
    pub consent: bool,
    pub payer: Option<azul_pay::Payer>,
}

/// Why Buy storage has no payment options: the token server refused this buyer (a country it
/// does not serve: the refusal in the app's words, never the server's sentence), or it could
/// not be asked.
pub(crate) enum OptionsFailed {
    Refused(Text),
    Failed(String),
}

/// Why Buy storage has no tiers: the token server refused this buyer (asking again does not
/// help: the refusal in the app's words), or it could not be asked (try again).
pub(crate) enum TiersFailed {
    Refused(Text),
    Failed(String),
}

/// The token server's refusal of this buyer (a country it does not serve, in the contract's word
/// `blocked_country`): asking again does not change it. In the app's words; `None` for any other
/// error - a 403 of an edge or a firewall is one of those: it gets "Try again" (AZ-P14).
pub(crate) fn buyer_refused(e: &TokenError) -> Option<Text> {
    e.is_blocked_country().then(|| token_error_text(e))
}

/// Why a checkout was not started: the words, and whether the prices on the page are stale (the
/// token server charges something else than they said), so they are asked for again.
pub(crate) struct StartFailed {
    pub why: Text,
    pub prices_moved: bool,
}

impl From<Text> for StartFailed {
    fn from(why: Text) -> StartFailed {
        StartFailed {
            why,
            prices_moved: false,
        }
    }
}

/// A checkout made: the token server's answer, the keyring's entry of it, and - through a
/// provider - its checked surface (`None`: the v1 checkout's payment page), or - cash by post -
/// its checked slip.
pub(crate) struct Started {
    pub checkout: Checkout,
    pub kept: PendingCheckout,
    pub created: Option<Created>,
    pub cash: Option<CashSlip>,
}

/// A Renew checkout's order and the drive's vaults, where its price lock is read.
pub(crate) struct RenewJob {
    pub order: crate::renewals::RenewOrder,
    pub store: PeriodTokenStore,
    pub azlin: Option<Arc<azcloud_kit::AzlinDrive>>,
}

/// A folder's size, counted for the Properties dialog.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(crate) struct FolderSize {
    pub bytes: u64,
    pub files: usize,
    pub folders: usize,
}

/// One blocking storage task.
pub(crate) enum Job {
    /// The open folder's rows, streamed in batches ([`Outcome::Scanned`]) until it is read or
    /// `cancel` is set (the window went elsewhere). A folder on this computer (`dir`) is read
    /// with one `read_dir` - names and kinds, no stat per entry; a bucket page by page.
    Scan {
        drive: Arc<dyn Drive>,
        /// The folder on this computer, for a local drive.
        dir: Option<PathBuf>,
        prefix: String,
        /// The listing asks the drive again (F5, after a change: [`ListRequest::refreshed`]).
        refresh: bool,
        serial: u64,
        cancel: Arc<AtomicBool>,
    },
    /// The size and date of `keys` (files and folders of a local drive whose root is `root`):
    /// the rows in view, or every row a sort by size waits for.
    Stat {
        root: PathBuf,
        keys: Vec<String>,
        serial: u64,
    },
    /// How many items the folders `keys` hold (`""` = the drive's root), each counted with one
    /// `read_dir` and no stat: the Size column of a folder, the details pane, This PC's tiles.
    Count {
        drive_id: String,
        root: PathBuf,
        keys: Vec<String>,
        /// Hidden items count only while they show.
        show_hidden: bool,
    },
    /// A tree node's folders: on this computer (`dir`) one `read_dir` with the kinds it reports
    /// (no stat per entry, so a folder of 100,000 files expands at once), else the bucket's
    /// listing page by page.
    Folders {
        drive: Arc<dyn Drive>,
        dir: Option<PathBuf>,
        node: TreeKey,
    },
    /// Plans transfer `id` (what to copy, which names are taken).
    Plan {
        id: u64,
        source: Arc<dyn Drive>,
        items: Vec<SourceItem>,
        target: Arc<dyn Drive>,
        target_prefix: String,
        kind: TransferKind,
        /// The window's language's word of a copy's name ("a - Copy.txt").
        copy: String,
    },
    /// Runs transfer `id`, sending its progress while it copies.
    Run {
        id: u64,
        plan: Plan,
        source: Arc<dyn Drive>,
        target: Arc<dyn Drive>,
        kind: TransferKind,
        cancel: Arc<AtomicBool>,
    },
    /// Into the trash folder (`stamp`), or for good (`None`).
    Delete {
        drive: Arc<dyn Drive>,
        drive_id: String,
        items: Vec<SourceItem>,
        stamp: Option<String>,
    },
    /// "Restore" on items of the local trash: each back where it was deleted from
    /// ([`fileops::restore_pairs`], [`fileops::restore_items`]).
    RestoreTrash {
        drive: Arc<dyn Drive>,
        drive_id: String,
        keys: Vec<String>,
    },
    /// The start's look at the trash of a drive on this computer ([`crate::trash::purge`]): marks
    /// what is old, removes what an earlier look marked a week ago (AZ-U16).
    PurgeTrash {
        drive_id: String,
        root: PathBuf,
        marks_file: PathBuf,
        now: u64,
    },
    Rename {
        drive: Arc<dyn Drive>,
        drive_id: String,
        from: String,
        to: String,
    },
    /// A new folder (`key` ends in `/`) or an empty file, then renamed in place.
    Create {
        drive: Arc<dyn Drive>,
        drive_id: String,
        key: String,
    },
    /// Ctrl+Z: renames back (`pairs` are (now, before)), or removes a new item.
    Undo {
        drive: Arc<dyn Drive>,
        pairs: Vec<(String, String)>,
        remove: Option<String>,
        trashed: bool,
    },
    Preview {
        drive: Arc<dyn Drive>,
        key: String,
        size: Option<u64>,
        kind: PreviewKind,
        /// Where a cloud video is fetched to ([`opened_copy`]).
        copies: Scratch,
    },
    /// Every object under a folder, counted.
    Measure {
        drive: Arc<dyn Drive>,
        prefix: String,
        serial: u64,
    },
    Metadata {
        drive: Arc<dyn Drive>,
        key: String,
    },
    /// Share > Zip: the items packed into `zip_key`.
    Zip {
        drive: Arc<dyn Drive>,
        items: Vec<SourceItem>,
        zip_key: String,
    },
    /// A file handed to the OS's app: where it is on this computer, else a read-only copy made
    /// in `copies` ([`opened_copy`]).
    Open {
        drive: Arc<dyn Drive>,
        key: String,
        copies: Scratch,
    },
    /// Add drive's "Test connection": the source `entry` opened with its keyring text
    /// `secret`, then ONE listing of its root, one entry at most.
    Test {
        serial: u64,
        entry: DriveEntry,
        secret: Option<String>,
    },
    /// A consumer cloud's sign-in: the authorization `code` (with the PKCE verifier and the
    /// exact redirect URI the sign-in went out with) exchanged for tokens at the plan's token
    /// endpoint - azul-storage's `oauth::exchange_code` through azul's HTTP client, to that
    /// endpoint only. Holds one-time secrets; never printed.
    OAuthExchange {
        serial: u64,
        plan: Box<crate::sign_in::SignInPlan>,
        code: String,
        code_verifier: String,
        redirect_uri: String,
    },
    /// Buy storage's tier list from the token server at `token_url`, priced for a buyer in
    /// `country` (empty: the ladder as it is).
    Tiers {
        serial: u64,
        token_url: String,
        country: String,
    },
    /// A test drive without payment (a development token server), its session into `keyring`
    /// under the drive's lock.
    CreateTestDrive {
        serial: u64,
        token_url: String,
        name: String,
        tier: String,
        keyring: SharedKeyring,
    },
    /// A checkout of `tier` for `months` months, its sign-up sealed to a new claim key, on the
    /// keyring's list of unfinished checkouts (with the drive's `name`) before its payment page
    /// opens - through `via`'s provider, method and surface (azul-pay), else the v1 checkout.
    Checkout {
        serial: u64,
        token_url: String,
        tier: String,
        months: u32,
        /// The method of a checkout without payment options (`via` none): one the tier is sold
        /// with.
        method: String,
        name: String,
        keyring: SharedKeyring,
        via: Option<Box<PayVia>>,
        /// Renew: the drive the months are for (its price lock read from its vaults).
        renew: Option<Box<RenewJob>>,
        /// What the buyer was shown for it: the checkout's charge is held to it before anything
        /// of the checkout is shown (`None`: no price was said).
        quote: Option<Quote>,
    },
    /// Buy storage's payment options (`GET /v1/checkout/options`) for `tier`, `months`, the
    /// payer's `country` and `currency`; `asked` the dialog's key of that order (its answer is
    /// taken only while the order is still that).
    Options {
        serial: u64,
        asked: String,
        token_url: String,
        tier: String,
        months: u32,
        country: String,
        currency: String,
    },
    /// The checkout `checkout_id` on the surface `kind` (`POST /v1/checkout/{id}/surface`),
    /// checked for `choice`.
    Surface {
        serial: u64,
        token_url: String,
        checkout_id: String,
        kind: SurfaceKind,
        choice: Box<Choice>,
        look: Look,
    },
    /// The checkout `checkout_id` abandoned (`POST /v1/checkout/{id}/abandon`): off the
    /// keyring's list when the token server ended it, kept in flight when not.
    Abandon {
        token_url: String,
        checkout_id: String,
        keyring: SharedKeyring,
    },
    /// The dialog's wait for `checkout`'s payment: its status asked every few seconds (at
    /// `token_url`, the checkout's) until the drive is the app's, the checkout ends, `cancel`
    /// is set or an hour is gone.
    AwaitPayment {
        serial: u64,
        checkout: PendingCheckout,
        token_url: String,
        keyring: SharedKeyring,
        cancel: Arc<AtomicBool>,
    },
    /// The background claims: the keyring's unfinished checkouts asked about every few seconds
    /// (each at its own token server, else `token_url`) until none is left or an hour is gone;
    /// a claimed one's period tokens issued into `store`. A cash checkout is asked once, and
    /// again only every `cash_every` seconds (the daily look's interval).
    Claims {
        keyring: SharedKeyring,
        token_url: Option<String>,
        store: PeriodTokenStore,
        /// The open Azlin drives by id: a claimed checkout's grant goes into its drive's vault.
        drives: HashMap<String, Arc<azcloud_kit::AzlinDrive>>,
        cash_every: u64,
    },
    /// A claimed checkout whose drive `drive_id` is in the drives file: without a `grant` it
    /// leaves the keyring's list; with one it stays there (its issue key with its claim secret)
    /// until its period tokens are issued at `token_url` and kept in `store`.
    FinishCheckout {
        keyring: SharedKeyring,
        store: PeriodTokenStore,
        /// The drive, when it is open already: its grant goes into its vault.
        azlin: Option<Arc<azcloud_kit::AzlinDrive>>,
        checkout: PendingCheckout,
        drive_id: String,
        grant: Option<PeriodTokens>,
        token_url: String,
    },
    /// Ends the drive `drive_id`'s monthly plan `checkout_id` with the issue key its vaults keep.
    CancelPlan {
        token_url: String,
        store: PeriodTokenStore,
        azlin: Option<Arc<azcloud_kit::AzlinDrive>>,
        drive_id: String,
        checkout_id: String,
    },
    /// A notice saved as a document in the drive (a price change: the durable medium).
    SaveNotice {
        azlin: Arc<azcloud_kit::AzlinDrive>,
        key: String,
        text: String,
    },
    /// "Get my drive back": proves the locked Azlin drive `drive_id` is this owner's - this
    /// computer's device key, or the recovery code typed - under its keyring lock with its
    /// newest drive token ([`crate::archive::prove`]).
    ArchiveProve {
        token_url: String,
        keyring: SharedKeyring,
        drive_id: String,
        via: crate::archive::ProveWith,
        /// The paid checkout the proof unlocks (its months count at once), when there is one.
        checkout: Option<String>,
    },
    /// This computer's own key of the Azlin drive `drive_id` registered for restores, once it
    /// holds the drive ([`crate::archive::register_device_key_now`]).
    RegisterDeviceKey {
        token_url: String,
        keyring: SharedKeyring,
        drive_id: String,
    },
    /// The copy of the read-only Azlin drive `drive_id` (`source`, opened) to this computer,
    /// into `folder`; `home`'s disk must have the room; `state` keeps how far it got
    /// ([`crate::archive::run_rescue`]).
    Rescue {
        drive_id: String,
        source: Arc<dyn Drive>,
        folder: PathBuf,
        home: PathBuf,
        state: Option<PathBuf>,
    },
    /// A look at the Azlin drives `drives` (each one's id and token server): their status (a
    /// pending recovery-key lockdown) and, when their periods near their ends, their next month
    /// from a period token kept in `store` - each under its drive's keyring lock, with its
    /// newest drive token ([`azcloud_kit::look_at_drive`]).
    RedeemPeriods {
        keyring: SharedKeyring,
        store: PeriodTokenStore,
        /// Each drive's id, its token server and, when it is open, the drive itself (its
        /// period tokens are kept in its vault then).
        drives: Vec<(String, String, Option<Arc<azcloud_kit::AzlinDrive>>)>,
    },
    /// The settings file saved by a merge (through a LocalDrive on the data tree): `mine` is
    /// what the window has, `start` what it last read from the file or saved to it - only what
    /// `mine` changed since is written, what another window changed stays (AZ-U7).
    SaveSettings {
        drive: LocalDrive,
        start: crate::model::Settings,
        mine: crate::model::Settings,
    },
    /// The pictures of the open folder as thumbnails for the icon layouts:
    /// (key, size) each, one `Thumbnail` answer per picture.
    Thumbnails {
        drive: Arc<dyn Drive>,
        items: Vec<(String, Option<u64>)>,
        max_px: u32,
    },
    /// The search box's search of a folder on this computer and every folder below it
    /// (azul-search): its results streamed in batches ([`Outcome::Searched`]) until it ends or
    /// `cancel` is set. It starts [`find::DEBOUNCE_MS`] after the key that asked for it.
    Find {
        serial: u64,
        request: azul_search::Request,
        /// The drive's key of the searched folder: the results' keys start with it.
        prefix: String,
        /// The drive's full-text index, asked first for the contents.
        index: Option<find::IndexAsk>,
        cancel: Arc<AtomicBool>,
    },
    /// The search box's search of a cloud drive's folder: the names of its listing (recursive -
    /// its folders side by side, the last listing first -, or its own level), page by page.
    FindRemote {
        find: RemoteFind,
        drive: Arc<dyn Drive>,
        cancel: Arc<AtomicBool>,
    },
    /// A drive's full-text index brought up to its files (what changed read again), its
    /// progress streamed ([`Outcome::IndexProgress`]) until it ends or `cancel`.
    IndexDrive {
        drive_id: String,
        source: IndexSource,
        dir: PathBuf,
        cancel: Arc<AtomicBool>,
    },
    /// A drive's index thrown away ("Index this drive" turned off).
    RemoveIndex { drive_id: String, dir: PathBuf },
    /// An encrypted drive's keys, recovery or files moved into the encryption.
    #[cfg(feature = "encryption")]
    Encryption(crate::encryption::EncryptionJob),
    /// What the node of the Azlin drive `azlin_id` counts of its bucket (one HeadBucket): the
    /// usage line's stored bytes and quota.
    BucketSpace {
        azlin_id: String,
        azlin: Arc<azcloud_kit::AzlinDrive>,
    },
    /// A voucher `code` at `token_url`: on `drive` (its id, under its keyring lock) the days it
    /// adds ([`Outcome::VoucherRedeemed`]); without one a new drive of `tier` (empty: the
    /// voucher's own), its session into the keyring like a test drive's ([`Outcome::Bought`] for
    /// the Add drive dialog `serial`, which names it).
    RedeemVoucher {
        serial: u64,
        token_url: String,
        code: String,
        tier: String,
        drive: Option<String>,
        keyring: SharedKeyring,
    },
    /// The device `member` of the Azlin drive `drive_id` removed from this computer, at the
    /// token server `token_url` ([`Outcome::DeviceRemoved`]).
    RemoveDevice {
        token_url: String,
        drive_id: String,
        member: String,
        keyring: SharedKeyring,
    },
    /// The folder sync: a pass, a file opened through it, pins, "Free up space", an answer.
    Sync(crate::sync_jobs::SyncJob),
    /// A round of drives' keyring entries, read one after the other (`unlock.rs`): quietly for
    /// the window's own work, asking for the user's Unlock.
    ReadKeys(crate::unlock::KeysRound),
    /// "Restore as of...": the Azlin drive `drive_id` put back as it was at `as_of` (seconds
    /// since 1970) - an encrypted one by its own restore (`encrypted`), else its bucket by the
    /// token server at `token_url` ([`crate::restore::run`]).
    RestoreDrive {
        drive_id: String,
        as_of: u64,
        token_url: String,
        keyring: SharedKeyring,
        encrypted: Option<crate::restore::EncryptedRestore>,
    },
    /// "Pick up a paid drive with a claim code": the cash checkout `checkout` (made of the
    /// code) onto `keyring`'s list of unfinished checkouts, for the Add drive dialog `serial`.
    PickUp {
        serial: u64,
        checkout: PendingCheckout,
        keyring: SharedKeyring,
    },
    /// "Join with a code": the join code's token spent once into `keyring` (this device's own
    /// session of the drive), at the token server `token_url`, for the Add drive dialog
    /// `serial`.
    Join {
        serial: u64,
        code: azcloud_kit::JoinCode,
        token_url: String,
        keyring: SharedKeyring,
    },
}

impl Job {
    /// Whether a close that cut this job off would leave data or a change half done - a delete
    /// half way, a rename that copied and did not delete, a copy of a drive that stopped, a
    /// restore the server was asked for: the first close waits for such a job and says so
    /// (`closing::work_running`). The match is exhaustive on purpose: a new kind of job does
    /// not compile until someone decides whether a close waits for it (AZ-U23, X-AR5a-20: a bulk
    /// delete, a bucket folder's rename and the rescue copy were cut mid-way because nobody
    /// had listed them).
    ///
    /// Not those the queue counts already (a transfer's plan and run), a sync pass (the sync
    /// view counts it), the dialog of an encryption, and the reads and waits that cut off cost
    /// nothing: a listing, a preview, a search, a payment's wait. A sync task decides for itself
    /// ([`SyncJob::cuts_data`]: a delete, a rename, an answered conflict, files freed or moved
    /// in).
    pub(crate) fn cuts_data(&self) -> bool {
        match self {
            Job::Delete { .. }
            | Job::PurgeTrash { .. }
            | Job::RestoreTrash { .. }
            | Job::Rename { .. }
            | Job::Create { .. }
            | Job::Undo { .. }
            | Job::Zip { .. }
            | Job::Rescue { .. }
            | Job::RestoreDrive { .. }
            | Job::SaveSettings { .. }
            | Job::SaveNotice { .. }
            | Job::CreateTestDrive { .. }
            | Job::Checkout { .. }
            | Job::Abandon { .. }
            | Job::FinishCheckout { .. }
            | Job::CancelPlan { .. }
            | Job::ArchiveProve { .. }
            | Job::RegisterDeviceKey { .. }
            | Job::RedeemPeriods { .. }
            | Job::RedeemVoucher { .. }
            | Job::RemoveDevice { .. }
            | Job::PickUp { .. }
            | Job::Join { .. } => true,
            Job::Scan { .. }
            | Job::Stat { .. }
            | Job::Count { .. }
            | Job::Folders { .. }
            | Job::Plan { .. }
            | Job::Run { .. }
            | Job::Preview { .. }
            | Job::Measure { .. }
            | Job::Metadata { .. }
            | Job::Open { .. }
            | Job::Test { .. }
            | Job::OAuthExchange { .. }
            | Job::Tiers { .. }
            | Job::Options { .. }
            | Job::Surface { .. }
            | Job::AwaitPayment { .. }
            | Job::Claims { .. }
            | Job::Thumbnails { .. }
            | Job::Find { .. }
            | Job::FindRemote { .. }
            | Job::IndexDrive { .. }
            | Job::RemoveIndex { .. }
            | Job::BucketSpace { .. }
            | Job::ReadKeys(_) => false,
            Job::Sync(job) => job.cuts_data(),
            #[cfg(feature = "encryption")]
            Job::Encryption(_) => false,
        }
    }
}

/// A search of a cloud drive's folder, as the window asks for it.
#[derive(Debug, Clone)]
pub(crate) struct RemoteFind {
    pub serial: u64,
    /// The searched folder's key (`""`: the drive's root): the results' keys start with it.
    pub prefix: String,
    pub pattern: azul_search::Pattern,
    pub options: find::FindOptions,
    /// The file that keeps the drive's last complete listing ([`find::listing_file`]); `None`:
    /// nothing is kept (a `--shot` run, a system without a cache folder).
    pub cache: Option<PathBuf>,
    /// An encrypted drive: its names are its drive index's (on this computer) - listed in one
    /// pass, never kept on disk.
    pub drive_index: bool,
    /// The drive's index, asked for the files' contents after the names.
    pub contents: Option<find::RemoteContents>,
}

/// What a drive's index reads.
#[derive(Clone)]
pub(crate) enum IndexSource {
    /// A folder on this computer, walked (a drive on this computer).
    Folder(PathBuf),
    /// A cloud or encrypted drive.
    Drive(DriveSource),
}

/// A drive whose files are not on this computer, as its index reads it: its listing (an
/// encrypted drive's: its drive index), each file from its local copy (the sync's), or - with
/// `download_cap` ("Index files that are not downloaded") - downloaded when it is at most that
/// large, read and dropped.
#[derive(Clone)]
pub(crate) struct DriveSource {
    pub drive: Arc<dyn Drive>,
    pub sync: Arc<dyn crate::sync_lookup::SyncLookup>,
    pub download_cap: Option<u64>,
}

/// What a job answers, on the UI thread.
pub(crate) enum Outcome {
    /// A batch of the open folder's rows; `done` with the last one (or the error that ended the
    /// scan).
    Scanned {
        serial: u64,
        batch: Vec<Entry>,
        done: bool,
        error: Option<azul_appkit::l10n::Text>,
    },
    /// The sizes and dates a [`Job::Stat`] found.
    Stats { serial: u64, stats: Vec<Stat> },
    /// The item counts a [`Job::Count`] found, by folder key.
    Counted {
        drive_id: String,
        counts: Vec<(String, usize)>,
    },
    /// A tree node's folders (sorted, without case) and how many items it holds in all, and
    /// (on this computer) which of those folders hold no folder: the tree's leaves.
    Folders {
        node: TreeKey,
        result: Result<(Vec<String>, usize), DriveError>,
        leaves: Vec<String>,
    },
    Planned {
        id: u64,
        result: Result<Plan, DriveError>,
    },
    /// A transfer's progress (several per transfer).
    Progress {
        id: u64,
        progress: Progress,
    },
    Ran {
        id: u64,
        report: TransferReport,
    },
    Deleted {
        drive_id: String,
        deleted: fileops::Deleted,
    },
    /// Items of the trash put back: how many, or why not.
    Restored {
        drive_id: String,
        result: Result<usize, DriveError>,
    },
    /// A look at a drive's trash ended.
    TrashPurged {
        drive_id: String,
        result: Result<crate::trash::Purged, String>,
    },
    Renamed {
        drive_id: String,
        from: String,
        to: String,
        result: Result<(), DriveError>,
    },
    Created {
        drive_id: String,
        key: String,
        result: Result<(), DriveError>,
    },
    Undone {
        result: Result<(), DriveError>,
    },
    Previewed {
        key: String,
        content: PreviewContent,
    },
    Measured {
        serial: u64,
        result: Result<FolderSize, DriveError>,
    },
    Metadata {
        key: String,
        result: Result<Vec<(String, String)>, DriveError>,
    },
    Zipped {
        zip_key: String,
        result: Result<u64, DriveError>,
    },
    Opened {
        key: String,
        result: Result<PathBuf, DriveError>,
        /// The user's own work found at the copy's place and put aside there: said to the user.
        kept_edit: Option<PathBuf>,
    },
    Tested {
        serial: u64,
        result: Result<String, DriveError>,
    },
    /// A sign-in's tokens (or why there are none). The tokens are secrets: never printed.
    SignedIn {
        serial: u64,
        result: Result<azul_storage::oauth::Tokens, String>,
    },
    /// Buy storage's tier list for a buyer in `country` (or why there is none).
    Tiers {
        serial: u64,
        country: String,
        result: Result<Tiers, TiersFailed>,
        /// The tiers closed for now.
        closed: Vec<String>,
    },
    /// A development server's test drive (or why there is none).
    Bought {
        serial: u64,
        result: Result<BoughtDrive, Text>,
    },
    /// A checkout, on the keyring's list of unfinished checkouts (or why there is none).
    CheckoutStarted {
        serial: u64,
        result: Result<Started, StartFailed>,
    },
    /// Buy storage's payment options for the order `asked`: the offer's text (`None`: the
    /// token server has none).
    Options {
        serial: u64,
        asked: String,
        result: Result<Option<String>, OptionsFailed>,
    },
    /// The checkout's next surface, checked (or why there is none).
    Surface {
        serial: u64,
        result: Result<azul_pay::Surface, Text>,
    },
    /// What the abandon of a checkout found.
    Abandoned {
        checkout_id: String,
        result: Abandoned,
    },
    /// The wait for a payment ended without a drive: why (empty: "Stop waiting" said it);
    /// `ended`: the checkout ended (declined, gone) - else it may still be paid.
    PaymentEnded { serial: u64, why: Text, ended: bool },
    /// A paid checkout's drive, its session in the keyring: from the dialog's wait (`serial`)
    /// or from the background claims (`None`: a message of a job that still runs).
    Claimed {
        serial: Option<u64>,
        checkout: PendingCheckout,
        claimed: Box<Claimed>,
    },
    /// The background claims took a checkout off the keyring's list (the payment declined, the
    /// token server no longer has it): why - to be said once. A job that still runs.
    CheckoutDropped { checkout_id: String, why: String },
    /// The background claims ended: what kept them from asking, if anything.
    ClaimsDone { problem: Option<Text> },
    /// A claimed checkout finished: `Ok(None)` off the keyring's list (no period tokens to
    /// issue), `Ok(Some(..))` what became of its period tokens, `Err` the list could not be
    /// changed. `from_claims`: a message of the background claims, which still run.
    CheckoutFinished {
        checkout_id: String,
        result: Result<Option<Finished>, String>,
        from_claims: bool,
    },
    /// What each drive's look found (drive id, its status and redemption).
    PeriodsRedeemed {
        results: Vec<(String, azcloud_kit::Look)>,
        /// Each drive's next months as the look saw them.
        renewals: Vec<(String, crate::renewals::RenewalSeen)>,
        /// The tier list of the looks' token server.
        news: Option<azcloud_kit::TierNews>,
    },
    /// A renewal's (or a tier change's) months were claimed for the drive `drive_id`; `record`:
    /// what became of the record of the payment (the months came first).
    Renewed {
        serial: Option<u64>,
        checkout_id: String,
        drive_id: String,
        months: u32,
        record: Recorded,
    },
    PlanCancelled { drive_id: String, result: Result<(), String> },
    /// A proof of a locked drive answered (its Azlin id), with the checkout it named.
    ArchiveProven {
        drive_id: String,
        checkout: Option<String>,
        result: Result<azcloud_kit::ProofAnswer, String>,
    },
    /// This computer's key of an Azlin drive registered for restores (its id).
    DeviceKeyRegistered {
        drive_id: String,
        result: Result<String, String>,
    },
    /// How far a read-only drive's copy to this computer got (its Azlin id).
    RescueProgress {
        drive_id: String,
        copied: usize,
        total: usize,
    },
    /// A read-only drive's copy to this computer ended.
    RescueDone {
        drive_id: String,
        report: crate::archive::Rescue,
    },
    NoticeSaved { key: String, result: Result<(), DriveError> },
    /// The settings saved (or why not); `sent` is what the window sent, which the next save is
    /// measured from.
    SettingsSaved {
        result: Result<(), DriveError>,
        sent: crate::model::Settings,
    },
    /// One picture's thumbnail (`None`: it could not be made).
    Thumbnail {
        key: String,
        image: Option<ImageRef>,
    },
    /// The thumbnails job ended.
    ThumbnailsDone,
    /// A batch of the search box's results; `end` with the last one.
    Searched {
        serial: u64,
        batch: Vec<find::Found>,
        phase: FindPhase,
        /// Files read (a cloud drive: keys listed) so far.
        searched: usize,
        end: Option<FindEnd>,
    },
    /// How far a drive's index update got, and what the index held when it began.
    IndexProgress {
        drive_id: String,
        progress: UpdateProgress,
        held: IndexStatus,
    },
    /// A drive's index update ended: what it did and what the index holds, or why it could not.
    Indexed {
        drive_id: String,
        result: Result<(UpdateSummary, IndexStatus), String>,
    },
    /// A drive's index was thrown away (or why it could not be).
    IndexRemoved {
        drive_id: String,
        error: Option<String>,
    },
    /// What an encryption job found.
    #[cfg(feature = "encryption")]
    Encryption(crate::encryption::EncryptionOutcome),
    /// The listing `serial` (of the drive in view) met a storage or token server error, as
    /// the user sees it ([`crate::problems`]); a message of a scan that still ends.
    DriveProblem { serial: u64, problem: UserError },
    /// The node's count of the Azlin drive `azlin_id` (or why there is none).
    BucketSpace {
        azlin_id: String,
        result: Result<azul_storage::BucketSpace, String>,
    },
    /// A voucher on `drive_id`: the days it added and the period's new end (seconds since
    /// 1970), or why not.
    VoucherRedeemed {
        drive_id: String,
        result: Result<(u32, Option<u64>), Text>,
        /// The days bought: the drive's credentials follow ([`azcloud_kit::AzlinDrive::apply`]).
        grant: Option<GrantChanged>,
    },
    /// What the removal of the device `member` from `drive_id` came to.
    DeviceRemoved {
        drive_id: String,
        member: String,
        result: Result<(), Text>,
    },
    /// What a sync job did (a pass's progress while it runs).
    Sync(crate::sync_jobs::SyncOutcome),
    /// What a [`Job::ReadKeys`] round read, drive by drive.
    KeysRead(crate::unlock::KeysRead),
    /// What "Restore as of..." of `drive_id` as of `as_of` came to, or why not.
    DriveRestored {
        drive_id: String,
        as_of: u64,
        result: Result<crate::restore::Restored, Text>,
    },
    /// The background claims asked about the cash checkout `checkout`: it awaits its letter. A
    /// job that still runs.
    CashWaiting { checkout: PendingCheckout },
    /// The background claims found `checkout`, bought here and not paid yet: its payment may
    /// be under way. A job that still runs.
    InFlight { checkout: PendingCheckout },
    /// A claim code's checkout is on the keyring's list (or why not), for the dialog `serial`.
    PickedUp {
        serial: u64,
        checkout: PendingCheckout,
        result: Result<(), String>,
    },
    /// A join code's drive on this device (its first answer and session), or why not.
    Joined {
        serial: u64,
        code: azcloud_kit::JoinCode,
        result: Result<azcloud_kit::Joined, String>,
    },
}

/// A thread's start data: the job, taken out once, and the claim a job that a close must wait
/// for holds until it ends ([`crate::closing::Inflight`]).
pub(crate) struct JobInit {
    pub job: Option<Job>,
    pub inflight: Option<crate::closing::Inflight>,
}

/// A thread's answer, taken out once by the write-back.
pub(crate) struct Done {
    pub outcome: Option<Outcome>,
    /// The claim of a job a close must wait for, carried with its LAST answer: it is held until
    /// the window has handled the answer ([`crate::on_job_done`]), so a close in the gap between
    /// the thread's end and the answer's read still waits - the answer of a bought drive writes
    /// its entry in the drives file (X-DELTA-AZDRIVE-FILES-30). An answer that is never read
    /// drops it with the message.
    pub inflight: Option<crate::closing::Inflight>,
}

/// An answer as the message the write-back takes: `inflight` is the claim it carries, if any.
pub(crate) fn answered(outcome: Outcome, inflight: Option<crate::closing::Inflight>) -> RefAny {
    RefAny::new(Done {
        outcome: Some(outcome),
        inflight,
    })
}

/// Sends `outcome` to the UI thread.
fn send(sender: &mut ThreadSender, outcome: Outcome) {
    send_message(sender, answered(outcome, None));
}

fn send_message(sender: &mut ThreadSender, message: RefAny) {
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        crate::on_job_done,
        message,
    )));
}

/// The bytes of a preview: the first 64 KB of a text, the whole image.
fn preview_bytes(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    kind: PreviewKind,
) -> Result<(Vec<u8>, bool), DriveError> {
    match kind {
        PreviewKind::Text => {
            let size = match size {
                Some(size) => size,
                None => drive.head(key)?.size,
            };
            if size == 0 {
                return Ok((Vec::new(), false));
            }
            let end = size.min(preview::TEXT_PREVIEW_BYTES) - 1;
            let bytes = drive.get_range(key, ByteRange::new(0, Some(end)))?;
            Ok((bytes, size > preview::TEXT_PREVIEW_BYTES))
        }
        _ => Ok((drive.get(key)?, false)),
    }
}

/// A PDF's first page as a picture: azul's PDF reader turns the page into
/// SVG, azul's SVG renderer draws it 560 px wide on white.
fn pdf_first_page(bytes: &[u8]) -> PreviewContent {
    use azul::{
        error::ResultParsedSvgSvgParseError,
        option::OptionColorU,
        svg::{ParsedSvg, SvgFitTo, SvgParseOptions, SvgRenderOptions},
    };
    let pages = azul::pdf::Pdf::create().to_svg_pages(azul::vec::U8VecRef::from(&bytes[..]));
    let Some(svg) = pages.as_slice().first().map(|s| s.as_str().to_string()) else {
        return PreviewContent::Message(Text::key("azdrive-preview-pdf-unreadable"));
    };
    let parsed = match ParsedSvg::from_string(svg, SvgParseOptions::create_default()) {
        ResultParsedSvgSvgParseError::Ok(parsed) => parsed,
        ResultParsedSvgSvgParseError::Err(_) => {
            return PreviewContent::Message(Text::key("azdrive-preview-pdf-not-drawn"))
        }
    };
    let mut options = SvgRenderOptions::create_default();
    options.fit = SvgFitTo::Width(560);
    options.background_color = OptionColorU::Some(ColorU {
        r: 255,
        g: 255,
        b: 255,
        a: 255,
    });
    let drawn = parsed.render(options).into_option().and_then(|image| {
        let (width, height) = (image.width, image.height);
        ImageRef::create_rawimage(image)
            .into_option()
            .map(|image| (image, width, height))
    });
    match drawn {
        Some((image, width, height)) => PreviewContent::Image {
            image,
            width,
            height,
        },
        None => PreviewContent::Message(Text::key("azdrive-preview-pdf-not-drawn")),
    }
}

/// How long a copy handed to an app is kept before a sweep removes it (at the start and before
/// the next copy): "swept after use" for a copy nobody can tell the end of use of.
pub(crate) const OPEN_COPY_KEPT: Duration = Duration::from_secs(24 * 3600);

/// The copy of the cloud file `key` an app (or the video preview) is handed (AZ-W4, AZ-U13,
/// AZ-U14). It was `<system temp>/AzDrive-open/<drive>/<name>`: a folder every user could read
/// and make first (a link planted there was followed), never cleaned, and a name that became
/// `name (1)` the next time. Now:
///
/// - in `copies` (scratch): a folder only this user can enter, checked, swept after a day;
/// - one folder per file AND VERSION (the key with the etag, size and date the drive reports
///   NOW): opened again, the same copy opens, and a changed file is a new copy;
/// - READ-ONLY: the product decision of U13 (Q17) - an edit of a copy nobody uploads was lost
///   silently; now the app that opens it cannot save in place and says so;
/// - put in place only if the name is still free ([`Expect::Absent`]): two windows opening the
///   same file make one copy.
fn opened_copy(drive: &dyn Drive, copies: &Scratch, key: &str) -> Result<PathBuf, DriveError> {
    opened_copy_keeping_edits(drive, copies, key).map(|opened| opened.path)
}

/// A copy handed to an app, and the user's work found where the copy goes.
struct OpenedCopy {
    path: PathBuf,
    /// A file that stood at the copy's place and was not the bytes AzDrive wrote - an editor that
    /// saves by renaming a new file over the read-only copy, "Save As" in its folder, an edit that
    /// kept the size - put aside under this path, never deleted (X-DELTA-AZDRIVE-FILES-7).
    kept_edit: Option<PathBuf>,
}

/// [`opened_copy`] that also says where it put the user's own work aside. The read-only mode stops
/// an in-place write, not an editor that saves a temp file and renames it over the copy (the
/// folder is the user's): that edit used to be deleted silently by the next Open of the file (the
/// length differed, the "changed" copy was removed and fetched again) or by the sweep a day later.
/// Now the copy's folder remembers what AzDrive wrote ([`Scratch::note_made`]); anything else at
/// the copy's place is put aside as `name (edited).ext` in that folder, which a sweep never takes
/// away while it holds it.
fn opened_copy_keeping_edits(
    drive: &dyn Drive,
    copies: &Scratch,
    key: &str,
) -> Result<OpenedCopy, DriveError> {
    let name = azul_storage::key::safe_file_name(key).ok_or_else(|| DriveError::InvalidKey {
        key: key.to_string(),
        reason: "it has no usable file name",
    })?;
    let head = drive.head(key)?;
    // A drive that reports neither an etag nor a date cannot tell one version from the next:
    // its copy is never reused.
    let version = match (&head.etag, head.modified) {
        (None, None) => format!("unversioned-{:x}", azul_storage::ids::random_seed()),
        (etag, modified) => format!("{}-{modified:?}", etag.as_deref().unwrap_or("")),
    };
    // What a run before left, and old copies of this one, go before a new copy comes.
    copies.sweep(OPEN_COPY_KEPT);
    let folder = copies
        .keep(&format!("{key}\0{version}\0{}", head.size))
        .map_err(|e| DriveError::Io(format!("{key}: {e}")))?;
    let dest = folder.join(&name);
    let whole = |path: &Path| fs::metadata(path).is_ok_and(|m| m.is_file() && m.len() == head.size);
    let untouched = || Scratch::made_state(&folder, &name) != MadeState::Changed;
    if whole(&dest) && untouched() {
        return Ok(OpenedCopy {
            path: dest,
            kept_edit: None,
        });
    }
    // Something that is not the bytes AzDrive wrote stands under the name (a copy somebody saved
    // over, shortened or edited): it is put aside, once, and the copy is fetched.
    let mut kept_edit = None;
    if fs::symlink_metadata(&dest).is_ok() {
        kept_edit = Some(keep_edit(&folder, &name)?);
    }
    let mut again = false;
    loop {
        match transfer::download_guarded(
            drive,
            key,
            Some(head.size),
            &dest,
            transfer::CHUNK,
            Expect::Absent,
            &Quiet,
        ) {
            Ok(_) => {
                // The system looks at a file from a drive before it runs it (AZ-W20); the copy of a
                // file that is already here was marked when it came.
                azul_storage::quarantine::mark_downloaded(&dest);
                break;
            }
            // Another window made the same copy in the same moment.
            Err(DriveError::Conflict { .. }) if whole(&dest) && untouched() => {
                return Ok(OpenedCopy {
                    path: dest,
                    kept_edit,
                })
            }
            // A file turned up under the name since the look: it is somebody's, put aside too.
            Err(DriveError::Conflict { .. }) if !again => {
                again = true;
                kept_edit = Some(keep_edit(&folder, &name)?);
            }
            Err(e) => return Err(e),
        }
    }
    // collapse-ok: a copy that stays writable is still private to this user; what a refusal
    // loses is only the loud failure of a save in place.
    let _ = scratch::make_read_only(&dest);
    // Noted last: the mode and the mark are part of the file as it is now. collapse-ok: with no
    // note a later look says "unknown" and takes the careful side (an unknown copy is reused only
    // when it is whole).
    let _ = Scratch::note_made(&folder, &name);
    Ok(OpenedCopy {
        path: dest,
        kept_edit,
    })
}

/// Puts the file `name` of `folder` aside as `<stem> (edited)<ext>` (then `(edited 2)`, ...),
/// never over a name that is taken: the user's own work, found where a copy goes. Its path.
fn keep_edit(folder: &Path, name: &str) -> Result<PathBuf, DriveError> {
    let io = |e: &dyn std::fmt::Display| DriveError::Io(format!("{name}: {e}"));
    let dir = Dir::open_following(folder).map_err(|e| io(&e))?;
    let (stem, extension) = fileops::split_extension(name);
    for n in 1..=999u32 {
        let aside = if n == 1 {
            format!("{stem} (edited){extension}")
        } else {
            format!("{stem} (edited {n}){extension}")
        };
        match dir.rename_noreplace(name, &dir, &aside) {
            Ok(()) => return Ok(folder.join(aside)),
            Err(e) if e.is_already_exists() => {}
            Err(e) => return Err(io(&e)),
        }
    }
    Err(io(&"a thousand edited copies are kept beside it already"))
}

/// A new empty file `key`, only if the name is free, in the one step the drive offers
/// ([`Drive::put_if`] with [`Precondition::Absent`]: a file made between the look and the write
/// is not replaced - X-AR5a-9); a drive that cannot ask looks first, as before.
fn create_empty_file(drive: &dyn Drive, key: &str) -> Result<(), DriveError> {
    let taken = || DriveError::InvalidKey {
        key: key.to_string(),
        reason: "something has this name already",
    };
    match drive.put_if(key, &[], &Precondition::Absent) {
        Ok(_) => Ok(()),
        Err(DriveError::Conflict { .. }) => Err(taken()),
        Err(DriveError::Unsupported(_)) => match storage_ops::exists(drive, key) {
            Ok(true) => Err(taken()),
            Ok(false) => drive.put(key, &[]),
            Err(e) => Err(e),
        },
        Err(e) => Err(e),
    }
}

/// The preview of `key`, made on the worker thread.
fn make_preview(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    kind: PreviewKind,
    copies: &Scratch,
) -> PreviewContent {
    // A row a scan listed has no size before its stat: the file's own, here (the limits below
    // read it).
    let size = size.or_else(|| {
        drive
            .local_path(key)
            .and_then(|path| fs::metadata(path).ok())
            .filter(fs::Metadata::is_file)
            .map(|meta| meta.len())
    });
    if kind == PreviewKind::Audio && preview::is_playable_audio(key) {
        if !size.is_some_and(|s| s <= preview::AUDIO_PREVIEW_MAX_BYTES) {
            return PreviewContent::Message(Text::key("azdrive-preview-wav-too-big"));
        }
        return match drive.get(key) {
            Err(e) => no_preview(drive_error_text(&e)),
            Ok(bytes) => match preview::wav_samples(&bytes) {
                Ok(wav) => PreviewContent::Audio(wav),
                Err(why) => PreviewContent::Message(Text::key(why)),
            },
        };
    }
    if let Some(reason) = preview::no_preview_reason(kind) {
        return PreviewContent::Message(Text::key(reason));
    }
    if kind == PreviewKind::Video {
        if let Some(path) = drive.local_path(key) {
            return PreviewContent::Video(path);
        }
        if !preview::fits_preview(kind, size) {
            return PreviewContent::Message(Text::key("azdrive-preview-video-too-big"));
        }
        return match opened_copy(drive, copies, key) {
            Ok(path) => PreviewContent::Video(path),
            Err(e) => no_preview(drive_error_text(&e)),
        };
    }
    if !preview::fits_preview(kind, size) {
        return PreviewContent::Message(Text::key("azdrive-preview-too-big"));
    }
    let (bytes, truncated) = match preview_bytes(drive, key, size, kind) {
        Ok(read) => read,
        Err(e) => return no_preview(drive_error_text(&e)),
    };
    match kind {
        PreviewKind::Text => match preview::text_preview(&bytes, truncated) {
            Ok(text) => PreviewContent::Text {
                text,
                cut: truncated,
            },
            Err(why) => PreviewContent::Message(Text::key(why)),
        },
        PreviewKind::Pdf => pdf_first_page(&bytes),
        PreviewKind::Image => {
            match RawImage::decode_image_bytes_any(U8VecRef::from(bytes.as_slice())) {
                azul::error::ResultRawImageDecodeImageError::Ok(image) => {
                    let (width, height) = (image.width, image.height);
                    match ImageRef::create_rawimage(image).into_option() {
                        Some(image) => PreviewContent::Image {
                            image,
                            width,
                            height,
                        },
                        None => {
                            PreviewContent::Message(Text::key("azdrive-preview-image-not-prepared"))
                        }
                    }
                }
                azul::error::ResultRawImageDecodeImageError::Err(_) => {
                    PreviewContent::Message(Text::key("azdrive-preview-image-undecodable"))
                }
            }
        }
        _ => PreviewContent::Message(Text::key("azdrive-preview-none")),
    }
}

/// Pictures bigger than this get no thumbnail (they are not fetched).
pub(crate) const THUMBNAIL_MAX_BYTES: u64 = 8 * 1024 * 1024;

/// One picture fetched, decoded and scaled down to `max_px`.
fn make_thumbnail(drive: &dyn Drive, key: &str, max_px: u32) -> Option<ImageRef> {
    let bytes = drive.get(key).ok()?;
    let image = match RawImage::decode_image_bytes_any(U8VecRef::from(bytes.as_slice())) {
        azul::error::ResultRawImageDecodeImageError::Ok(image) => image,
        azul::error::ResultRawImageDecodeImageError::Err(_) => return None,
    };
    let thumbnail = image.thumbnail(max_px, max_px).into_option()?;
    ImageRef::create_rawimage(thumbnail).into_option()
}

/// Every object under `prefix`, counted: bytes, files, folders.
fn measure(drive: &dyn Drive, prefix: &str) -> Result<FolderSize, DriveError> {
    let mut size = FolderSize::default();
    let mut folders = std::collections::HashSet::new();
    for object in storage_ops::list_all(drive, prefix)? {
        let rest = &object.key[prefix.len().min(object.key.len())..];
        // Every folder on the way to the object (markers and implied ones).
        let mut at = 0;
        while let Some(i) = rest[at..].find('/') {
            folders.insert(rest[..at + i].to_string());
            at += i + 1;
        }
        if !object.key.ends_with('/') {
            size.files += 1;
            size.bytes += object.size;
        }
    }
    size.folders = folders.len();
    Ok(size)
}

/// The most a Share > Zip builds in memory: the archive is made whole before it is put (the zip
/// writer of the toolkit has no stream), so a bigger folder is refused with a sentence instead of
/// exhausting the computer's memory (X-DELTA-AZDRIVE-FILES-21).
pub(crate) const ZIP_IN_MEMORY_MAX: u64 = 512 << 20;

/// Share > Zip: every file of `items` (folders recursively) into one zip, put at `zip_key` or - if
/// that name was taken since the window chose it (the listing it chose from was loaded before another
/// device saved `Photos.zip`, or is still loading) - at the next free name. The key it went to, and
/// the archive's size.
fn make_zip(
    drive: &dyn Drive,
    items: &[SourceItem],
    zip_key: &str,
) -> Result<(u64, String), DriveError> {
    // What goes in: the sizes first, so a folder too big for memory is refused before a byte is read.
    let mut folders: Vec<String> = Vec::new();
    let mut files: Vec<(String, String)> = Vec::new();
    let mut total = 0u64;
    for item in items {
        let base = fileops::parent_of(&item.key);
        if item.is_folder {
            folders.push(item.key[base.len()..].to_string());
            for object in storage_ops::list_all(drive, &item.key)? {
                let name = object.key[base.len()..].to_string();
                if object.key.ends_with('/') {
                    folders.push(name);
                } else {
                    total += object.size;
                    files.push((object.key, name));
                }
            }
        } else {
            total += match item.size {
                Some(size) => size,
                None => drive.head(&item.key)?.size,
            };
            files.push((item.key.clone(), item.key[base.len()..].to_string()));
        }
    }
    if total > ZIP_IN_MEMORY_MAX {
        return Err(DriveError::Unsupported(format!(
            "a zip of more than {} MiB (it is built in memory); zip a smaller folder",
            ZIP_IN_MEMORY_MAX >> 20
        )));
    }
    let mut zip = azul::zip::Zip::create();
    for folder in folders {
        zip.add_directory(folder);
    }
    for (key, name) in files {
        zip.add_file(name, drive.get(&key)?);
    }
    let bytes: Vec<u8> = zip.to_bytes().as_slice().to_vec();
    let len = bytes.len() as u64;
    let key = put_new_zip(drive, zip_key, &bytes)?;
    Ok((len, key))
}

/// Puts `bytes` at `wanted` only if the name is free in the one step the drive offers
/// ([`Drive::put_if`] with [`Precondition::Absent`]); a name that is taken - by a file that turned
/// up since the window looked - gets the next one (`Photos (2).zip`); a drive that cannot ask
/// looks first. Never over a file that is there (X-DELTA-AZDRIVE-FILES-21, the twin of X-AR5a-9).
fn put_new_zip(drive: &dyn Drive, wanted: &str, bytes: &[u8]) -> Result<String, DriveError> {
    let (prefix, name) = match wanted.rfind('/') {
        Some(slash) => wanted.split_at(slash + 1),
        None => ("", wanted),
    };
    let mut key = wanted.to_string();
    // The names found taken, so one the drive does not show (a conflict it answered) is not tried again.
    let mut tried: Vec<String> = Vec::new();
    for _ in 0..1000 {
        match drive.put_if(&key, bytes, &Precondition::Absent) {
            Ok(_) => return Ok(key),
            // Taken since the window chose it: the next name.
            Err(DriveError::Conflict { .. }) => {}
            // A drive that cannot ask: looked at, then written.
            Err(DriveError::Unsupported(_)) => {
                if !storage_ops::exists(drive, &key)? {
                    drive.put(&key, bytes)?;
                    return Ok(key);
                }
            }
            Err(e) => return Err(e),
        }
        tried.push(key.clone());
        let other = fileops::new_name(name, &|candidate: &str| {
            // A name found taken, or one the drive has. collapse-ok: a look that fails reads as
            // "taken" - the careful side; a drive that cannot be looked at ends in the error below.
            let full = format!("{prefix}{candidate}");
            tried.contains(&full) || storage_ops::exists(drive, &full).unwrap_or(true)
        });
        key = format!("{prefix}{other}");
    }
    Err(DriveError::InvalidKey {
        key: wanted.to_string(),
        reason: "a thousand files of this name are there already",
    })
}

/// The path of `key` (a file, or a folder ending in `/`; `""` the root) under a local drive's
/// `root`. The keys come from the drive's own scan: their segments are directory entry names.
pub(crate) fn path_in(root: &Path, key: &str) -> PathBuf {
    let mut path = root.to_path_buf();
    for segment in key.split('/').filter(|s| !s.is_empty()) {
        path.push(segment);
    }
    path
}

/// What a directory entry is, from the kind the directory reports (no stat): a folder, a file,
/// or nothing to show (`None`: a socket, a device, a broken link). A symbolic link is followed
/// to what it points at, as the drive's own listing does.
fn entry_kind(entry: &fs::DirEntry) -> Option<bool> {
    let kind = entry.file_type().ok()?;
    if kind.is_symlink() {
        let meta = fs::metadata(entry.path()).ok()?;
        return if meta.is_dir() {
            Some(true)
        } else {
            meta.is_file().then_some(false)
        };
    }
    if kind.is_dir() {
        Some(true)
    } else {
        kind.is_file().then_some(false)
    }
}

/// The scan of the folder `dir` on this computer (the drive's `prefix`): its rows in batches,
/// the first as soon as it holds [`listing::FIRST_BATCH`] rows (the first screen shows at
/// once), then one every [`listing::BATCH_MS`]; the answer is the last batch, `done`. The
/// folder is read in batches without a stat per entry (`azul_storage::dir_read`): on macOS the
/// rows come with their sizes and dates, elsewhere with names and kinds only and the sizes and
/// dates of the rows in view come from [`Job::Stat`]. A set `cancel` (the window went
/// elsewhere) ends it between two batches of the read.
fn scan_dir(
    dir: &Path,
    prefix: &str,
    serial: u64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let at_root = prefix.is_empty();
    let mut batch = Vec::with_capacity(listing::FIRST_BATCH);
    let mut sent_first = false;
    let mut last = Instant::now();
    let mut cancelled = false;
    let read = azul_storage::dir_read::read_dir_items(dir, &mut |items| {
        if cancel.load(Ordering::Relaxed) {
            cancelled = true;
            return std::ops::ControlFlow::Break(());
        }
        for item in items {
            if !listing::listed_name(&item.name, at_root) {
                continue;
            }
            batch.push(listing::scanned_item(prefix, &item));
            let due = if sent_first {
                last.elapsed() >= Duration::from_millis(listing::BATCH_MS)
            } else {
                batch.len() >= listing::FIRST_BATCH
            };
            if due {
                sent_first = true;
                last = Instant::now();
                emit(Outcome::Scanned {
                    serial,
                    batch: std::mem::take(&mut batch),
                    done: false,
                    error: None,
                });
            }
        }
        std::ops::ControlFlow::Continue(())
    });
    if cancelled {
        batch.clear();
    }
    Outcome::Scanned {
        serial,
        batch,
        done: true,
        error: read.err().map(|e| e.to_string().into()),
    }
}

/// The scan of a bucket's folder: page after page, each page a batch, until the last page or a
/// set `cancel`.
/// The scan's request for a page of `prefix` (after `continuation`): a refresh's first page asks
/// the drive again ([`ListRequest::refreshed`]: an encrypted drive's index pulls before it
/// answers), the next pages continue from what that read.
fn scan_request(prefix: &str, continuation: Option<String>, refresh: bool) -> ListRequest {
    let request = ListRequest::folder(prefix).with_max_keys(SCAN_PAGE);
    match continuation {
        Some(token) => request.with_continuation(token),
        None if refresh => request.refreshed(),
        None => request,
    }
}

fn scan_bucket(
    drive: &dyn Drive,
    prefix: &str,
    refresh: bool,
    serial: u64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let mut next: Option<String> = None;
    loop {
        let request = scan_request(prefix, next.take(), refresh);
        let page = match drive.list(&request) {
            Ok(page) => page,
            Err(e) => {
                // A storage or token server error in the table's words, with its error ID.
                if let Some(problem) = UserError::from_drive_error(&e) {
                    emit(Outcome::DriveProblem { serial, problem });
                }
                return Outcome::Scanned {
                    serial,
                    batch: Vec::new(),
                    done: true,
                    error: Some(crate::l10n::drive_error_text(&e)),
                };
            }
        };
        let batch = browse::entries_of(&page, prefix);
        if cancel.load(Ordering::Relaxed) {
            return Outcome::Scanned {
                serial,
                batch: Vec::new(),
                done: true,
                error: None,
            };
        }
        match page.next {
            Some(token) => {
                emit(Outcome::Scanned {
                    serial,
                    batch,
                    done: false,
                    error: None,
                });
                next = Some(token);
            }
            None => {
                return Outcome::Scanned {
                    serial,
                    batch,
                    done: true,
                    error: None,
                }
            }
        }
    }
}

/// The sizes and dates of the local items `keys` (a [`Job::Stat`]'s answer): one answer per key,
/// a key whose stat fails (gone since the scan, a dangling link) with neither size nor date -
/// answered all the same, so a sort that waits for every row is not held up by it.
fn stats_of(root: &Path, keys: &[String]) -> Vec<Stat> {
    keys.iter()
        .map(|key| {
            stat_of(root, key).unwrap_or_else(|| Stat {
                key: key.clone(),
                size: None,
                modified: None,
            })
        })
        .collect()
}

/// A local item's size and date (`fs::metadata` follows a symbolic link, as the drive's own
/// listing does); `None` for one gone since the scan.
fn stat_of(root: &Path, key: &str) -> Option<Stat> {
    let meta = fs::metadata(path_in(root, key)).ok()?;
    Some(Stat {
        key: key.to_string(),
        size: meta.is_file().then(|| meta.len()),
        modified: meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
            .map(|d| d.as_secs()),
    })
}

/// How many items the local folder `key` holds, with one `read_dir` and no stat: what a scan
/// would list (hidden items only when shown).
fn count_items(root: &Path, key: &str, show_hidden: bool) -> Option<usize> {
    let entries = fs::read_dir(path_in(root, key)).ok()?;
    let at_root = key.is_empty();
    Some(
        entries
            .filter_map(Result::ok)
            .filter(|entry| {
                entry.file_name().to_str().is_some_and(|name| {
                    listing::listed_name(name, at_root) && (show_hidden || !name.starts_with('.'))
                })
            })
            .count(),
    )
}

/// The most pages a tree node's listing of a bucket reads (a folder's subfolders come with its
/// first keys; a folder of millions of objects is not walked to its end to expand a node).
const TREE_PAGES: usize = 20;

/// Whether the folder at `dir` on this computer holds a folder: one `read_dir`, ended at the
/// first one. A hidden folder counts (the tree may show it), a temporary file's never does.
pub(crate) fn has_subfolder(dir: &Path) -> bool {
    let Ok(entries) = fs::read_dir(dir) else {
        return false;
    };
    entries.flatten().any(|entry| {
        entry
            .file_name()
            .to_str()
            .is_some_and(|name| listing::listed_name(name, false))
            && entry_kind(&entry) == Some(true)
    })
}

/// The most folders of one listing whose own folders are looked for (a folder of thousands of
/// folders keeps its expanders until each is opened).
const LEAF_CHECKS: usize = 256;

/// A tree node's folders, sorted without case, and how many items the node holds: on this
/// computer one `read_dir` with the kinds it reports - no stat per entry -, in a bucket its
/// listing page by page.
fn tree_folders(
    drive: &dyn Drive,
    dir: Option<&Path>,
    node: &TreeKey,
) -> Result<(Vec<String>, usize), DriveError> {
    let prefix = node.1.as_str();
    let mut folders = Vec::new();
    let mut items = 0;
    match dir {
        Some(dir) => {
            for entry in fs::read_dir(dir)? {
                let Ok(entry) = entry else {
                    continue;
                };
                let Some(name) = entry.file_name().to_str().map(str::to_string) else {
                    continue;
                };
                if !listing::listed_name(&name, prefix.is_empty()) {
                    continue;
                }
                match entry_kind(&entry) {
                    Some(true) => {
                        items += 1;
                        folders.push(format!("{prefix}{name}/"));
                    }
                    Some(false) => items += 1,
                    None => {}
                }
            }
        }
        None => {
            let mut next: Option<String> = None;
            for _ in 0..TREE_PAGES {
                let mut request = ListRequest::folder(prefix).with_max_keys(SCAN_PAGE);
                if let Some(token) = next.take() {
                    request = request.with_continuation(token);
                }
                let page = drive.list(&request)?;
                items += page.folders.len()
                    + page
                        .objects
                        .iter()
                        .filter(|o| o.key != prefix && !o.key.ends_with('/'))
                        .count();
                folders.extend(page.folders);
                match page.next {
                    Some(token) => next = Some(token),
                    None => break,
                }
            }
        }
    }
    folders.sort_by_key(|f| f.to_lowercase());
    Ok((folders, items))
}

/// Ctrl+Z of a new folder or file: it goes only while it is still what
/// was created - an empty folder, an empty file - never with what the user
/// put in since.
fn undo_create(drive: &dyn Drive, key: &str) -> Result<(), DriveError> {
    let no_longer_new = || DriveError::InvalidKey {
        key: key.to_string(),
        reason: "it is not empty any more; delete it instead",
    };
    if key.ends_with('/') {
        // The drive removes a folder only when nothing is in it ([`Drive::remove_empty_folder`]):
        // a listing cannot see a socket, a link to a folder or a name that is not UTF-8 (an unzip
        // of a Latin-1 archive), and `delete_folder` after "the listing shows nothing" removed
        // them with the folder (X-DELTA-AZDRIVE-FILES-20, the twin of X-AR5a-10).
        return if drive.remove_empty_folder(key)? {
            Ok(())
        } else {
            Err(no_longer_new())
        };
    }
    let info = drive.head(key)?;
    if info.size != 0 {
        return Err(no_longer_new());
    }
    // Deleted only while it is still the empty file that was looked at: one written to between
    // the look and the delete is the user's.
    let item = match &info.etag {
        Some(tag) => BatchDelete::new(key).of_version(tag),
        None => BatchDelete::new(key),
    };
    match drive.delete_many(&[item]).into_iter().next() {
        Some(Err(DriveError::Conflict { .. })) => Err(no_longer_new()),
        Some(other) => other,
        None => Ok(()),
    }
}

// ==== The search box's search ====

/// A search's results on their way to the window: handed over as soon as a first screen's worth
/// is in, then every [`listing::BATCH_MS`] - a batch rebuilds the window - and when a walk
/// begins (the status line says which).
struct FindBatch {
    serial: u64,
    items: Vec<find::Found>,
    phase: FindPhase,
    searched: usize,
    last: Instant,
    sent_first: bool,
}

impl FindBatch {
    fn new(serial: u64) -> FindBatch {
        FindBatch {
            serial,
            items: Vec::new(),
            phase: FindPhase::Waiting,
            searched: 0,
            last: Instant::now(),
            sent_first: false,
        }
    }

    fn outcome(&mut self, end: Option<FindEnd>) -> Outcome {
        self.last = Instant::now();
        Outcome::Searched {
            serial: self.serial,
            batch: std::mem::take(&mut self.items),
            phase: self.phase,
            searched: self.searched,
            end,
        }
    }

    /// Hands the results over when they are due.
    fn tick(&mut self, emit: &mut dyn FnMut(Outcome)) {
        let due = !self.items.is_empty()
            && (self.last.elapsed() >= Duration::from_millis(listing::BATCH_MS)
                || (!self.sent_first && self.items.len() >= find::FIRST_BATCH));
        if due {
            self.sent_first = true;
            let outcome = self.outcome(None);
            emit(outcome);
        }
    }

    fn push(&mut self, found: find::Found, emit: &mut dyn FnMut(Outcome)) {
        self.items.push(found);
        self.tick(emit);
    }

    /// What was found so far goes along now (the walk it was found in is about to change).
    fn flush(&mut self, emit: &mut dyn FnMut(Outcome)) {
        if !self.items.is_empty() {
            self.sent_first = true;
            let outcome = self.outcome(None);
            emit(outcome);
        }
    }

    /// A walk begins: what was found so far goes along.
    fn phase(&mut self, phase: FindPhase, emit: &mut dyn FnMut(Outcome)) {
        self.phase = phase;
        let outcome = self.outcome(None);
        emit(outcome);
    }

    /// The last batch, with how the search ended.
    fn finish(mut self, end: FindEnd) -> Outcome {
        self.outcome(Some(end))
    }
}

/// Waits [`find::DEBOUNCE_MS`] for the typing to pause, in short steps: `false` when the search
/// was stopped meanwhile (another key came).
fn debounce(cancel: &AtomicBool) -> bool {
    for _ in 0..find::DEBOUNCE_MS / 10 {
        if cancel.load(Ordering::Relaxed) {
            return false;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    !cancel.load(Ordering::Relaxed)
}

/// The search box's search of a folder on this computer and every folder below it
/// (azul-search: the names, then the contents when asked): its results in batches through
/// `emit` as the rows of the drive (keys under `prefix`), the last batch in the answer.
pub(crate) fn run_find(
    serial: u64,
    request: &azul_search::Request,
    prefix: &str,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let mut batch = FindBatch::new(serial);
    let result = azul_search::search(request, cancel, &mut |event| match event {
        azul_search::Event::Phase(azul_search::Phase::Names) => {
            batch.phase(FindPhase::Names, &mut *emit);
        }
        azul_search::Event::Phase(azul_search::Phase::Contents) => {
            batch.phase(FindPhase::Contents, &mut *emit);
        }
        azul_search::Event::Name(hit) => batch.push(find::found_name(prefix, hit), &mut *emit),
        azul_search::Event::Content(hit) => {
            batch.push(find::found_content(prefix, hit), &mut *emit);
        }
        azul_search::Event::Progress(progress) => {
            batch.searched = progress.searched;
            batch.tick(&mut *emit);
        }
    });
    let end = match result {
        Ok(summary) => FindEnd {
            limited: summary.limited,
            ..FindEnd::default()
        },
        Err(e) => FindEnd {
            error: Some(e.to_string()),
            ..FindEnd::default()
        },
    };
    batch.finish(end)
}

/// The search box's search of a cloud drive's folder: the names of its listing - recursive, or
/// ("Current folder") its own level -, page by page (slower than a folder on this computer; no
/// contents - the files would have to be downloaded), refined by the sizes and dates the listing
/// has, at most [`find::FIND_MAX`]; a set `cancel` ends it between pages.
///
/// A recursive one lists the folder's own level, then its subfolders side by side
/// ([`list_side_by_side`]). The drive's last complete listing (`search.cache`) goes first: its
/// results at once ([`FindPhase::Cached`]), then the fresh listing's others; once the fresh one
/// is complete the cached results it has not got end the search as stale, and it is kept for
/// the next search. An encrypted drive's names (`search.drive_index`) are listed in one pass
/// from its drive index, and nothing of them is kept. With the drive's index (`search.contents`)
/// the files whose text holds the words follow the names ([`drive_contents`]).
pub(crate) fn run_find_remote(
    search: &RemoteFind,
    drive: &dyn Drive,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let mut batch = FindBatch::new(search.serial);
    let matcher = match azul_search::NameMatcher::new(&search.pattern) {
        Ok(matcher) => matcher,
        Err(e) => {
            return batch.finish(FindEnd {
                error: Some(e.to_string()),
                ..FindEnd::default()
            })
        }
    };
    let prefix = search.prefix.as_str();
    let options = &search.options;
    let mut found = 0;
    // An encrypted drive's names are its drive index's: nothing of them is kept on disk.
    let cache = search.cache.as_deref().filter(|_| !search.drive_index);
    // The last complete listing, when it covers this folder (a recursive search only).
    let cached = cache
        .filter(|_| options.subfolders)
        .and_then(find::read_listing);
    let mut shown_cached = HashSet::new();
    if let Some(listing) = cached
        .as_ref()
        .filter(|listing| prefix.starts_with(listing.prefix.as_str()))
    {
        batch.phase(FindPhase::Cached, emit);
        let page = ListPage {
            objects: listing
                .objects
                .iter()
                .filter(|o| o.key.starts_with(prefix))
                .cloned()
                .collect(),
            ..ListPage::default()
        };
        let mut seen = HashSet::new();
        for item in find::remote_names(&page, prefix, &matcher, options, &mut seen) {
            if found == find::FIND_MAX {
                break;
            }
            found += 1;
            shown_cached.insert(item.entry.key.clone());
            batch.push(item, emit);
        }
        batch.flush(emit);
    }
    batch.phase(FindPhase::Names, emit);
    // The fresh listing: a cached result is not handed over again; every result it has is
    // `live`; its objects are gathered for the cache (while they fit).
    let mut live = HashSet::new();
    let mut seen = HashSet::new();
    let mut limited = false;
    let mut listed: Option<Vec<ObjectInfo>> =
        (cache.is_some() && options.subfolders).then(Vec::new);
    // The sizes and dates of the files, for the rows the index finds by their contents.
    let mut facts: HashMap<String, ObjectInfo> = HashMap::new();
    let want_facts = search.contents.is_some() && options.contents;
    let mut on_page = |page: ListPage| -> bool {
        batch.searched += page.objects.len();
        if want_facts {
            for object in page.objects.iter().filter(|o| !o.key.ends_with('/')) {
                facts.insert(object.key.clone(), object.clone());
            }
        }
        for item in find::remote_names(&page, prefix, &matcher, options, &mut seen) {
            if !live.insert(item.entry.key.clone()) || shown_cached.contains(&item.entry.key) {
                continue;
            }
            if found == find::FIND_MAX {
                limited = true;
                return false;
            }
            found += 1;
            batch.push(item, &mut *emit);
        }
        let fits = listed.as_ref().is_some_and(|objects| {
            objects.len() + page.objects.len() <= find::CACHE_MAX_OBJECTS
        });
        if fits {
            if let Some(objects) = listed.as_mut() {
                objects.extend(page.objects);
            }
        } else {
            listed = None;
        }
        batch.tick(&mut *emit);
        true
    };
    let listing = if !options.subfolders {
        list_pages(drive, &ListRequest::folder(prefix), cancel, &mut Vec::new(), &mut on_page)
    } else if search.drive_index {
        // The drive index answers on this computer: one recursive listing.
        list_pages(drive, &ListRequest::recursive(prefix), cancel, &mut Vec::new(), &mut on_page)
    } else {
        list_side_by_side(drive, prefix, cancel, &mut on_page)
    };
    let mut end = match listing {
        Err(error) => FindEnd {
            error: Some(error),
            ..FindEnd::default()
        },
        Ok(_) if limited => FindEnd {
            limited: true,
            ..FindEnd::default()
        },
        Ok(Listed::Stopped) => FindEnd::default(),
        Ok(Listed::Complete) => {
            let mut stale: Vec<String> = shown_cached.difference(&live).cloned().collect();
            stale.sort();
            if let (Some(file), Some(objects)) = (cache, listed) {
                keep_listing(file, cached, prefix, objects);
            }
            FindEnd {
                stale,
                ..FindEnd::default()
            }
        }
    };
    // The files whose text holds the words, from the drive's index (the names first).
    if let Some(contents) = search.contents.as_ref().filter(|_| want_facts) {
        if end.error.is_none() && !end.limited && !cancel.load(Ordering::Relaxed) {
            end.limited = drive_contents(search, contents, &facts, found, &mut batch, emit);
        }
    }
    batch.finish(end)
}

/// The contents part of a search of a cloud or encrypted drive with an index (`contents`): the
/// files below the folder whose text holds the search's words, best first - a local copy's line
/// read there, any other without a line (its text is in the index only) -, with the sizes and
/// dates of the names' listing (`facts`: a file not in it is gone since), the refine, hidden
/// items and "Current folder" holding; at most [`find::FIND_MAX`] results in all (`found` so
/// far). Whether the limit stopped it.
fn drive_contents(
    search: &RemoteFind,
    contents: &find::RemoteContents,
    facts: &HashMap<String, ObjectInfo>,
    mut found: usize,
    batch: &mut FindBatch,
    emit: &mut dyn FnMut(Outcome),
) -> bool {
    let text = search.pattern.text.as_str();
    // The words as the walk's contents search takes them (a glob names files: no contents).
    let Ok(matcher) = azul_search::ContentMatcher::new(&azul_search::Pattern::literal(text)) else {
        return false;
    };
    let asked = DriveIndex::open(&contents.dir)
        .and_then(|index| index.query(text, &search.prefix, find::FIND_MAX));
    let paths = match asked {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("[azdrive] the index in {} is passed by: {e}", contents.dir.display());
            return false;
        }
    };
    batch.phase(FindPhase::Contents, emit);
    let options = &search.options;
    let extractors = extractors();
    for key in paths {
        let Some(rel) = key.strip_prefix(search.prefix.as_str()) else {
            continue;
        };
        let Some(object) = facts.get(&key) else {
            continue; // gone since the index read it
        };
        if (!options.show_hidden && find::hidden_path(rel))
            || (!options.subfolders && rel.contains('/'))
            || !options.refine.admits_name(key_name(rel), false)
            || !options.refine.admits_facts(Some(object.size), object.modified)
        {
            continue;
        }
        if found >= find::FIND_MAX {
            return true;
        }
        let line = contents
            .sync
            .local_copy(&contents.drive_id, &key)
            .and_then(|copy| azul_search_index::file_text(&copy, &extractors))
            .and_then(|text| find::document_line(&text, &matcher));
        found += 1;
        let row = find::found_document(&search.prefix, rel, object.size, object.modified, line);
        batch.push(row, emit);
    }
    false
}

/// How a cloud drive's listing ended.
enum Listed {
    /// Every page was listed.
    Complete,
    /// Cancelled, or the pages' reader asked to stop (the search's limit).
    Stopped,
}

/// A listing of a drive (`base`: one folder's level, or a folder and everything below it), page
/// by page through `on_page` (`false`: stop); the subfolders of a level's listing (the common
/// prefixes) gathered into `folders`.
fn list_pages(
    drive: &dyn Drive,
    base: &ListRequest,
    cancel: &AtomicBool,
    folders: &mut Vec<String>,
    on_page: &mut dyn FnMut(ListPage) -> bool,
) -> Result<Listed, String> {
    let mut next: Option<String> = None;
    loop {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Listed::Stopped);
        }
        let mut request = base.clone().with_max_keys(SCAN_PAGE);
        if let Some(token) = next.take() {
            request = request.with_continuation(token);
        }
        let page = drive.list(&request).map_err(|e| e.to_string())?;
        folders.extend(page.folders.iter().cloned());
        let more = page.next.clone();
        if !on_page(page) {
            return Ok(Listed::Stopped);
        }
        match more {
            Some(token) => next = Some(token),
            None => return Ok(Listed::Complete),
        }
    }
}

/// A cloud drive's folder `prefix` and every folder below it: its own level first, then each
/// subfolder's recursive listing on one of [`LIST_WORKERS`] threads side by side (a bucket
/// lists one key range per request, page after page; the folders are independent ranges), every
/// page handed to `on_page` on this thread as it comes (`false`: the workers stop).
fn list_side_by_side(
    drive: &dyn Drive,
    prefix: &str,
    cancel: &AtomicBool,
    on_page: &mut dyn FnMut(ListPage) -> bool,
) -> Result<Listed, String> {
    let mut folders = Vec::new();
    let level = ListRequest::folder(prefix);
    if let Listed::Stopped = list_pages(drive, &level, cancel, &mut folders, on_page)? {
        return Ok(Listed::Stopped);
    }
    if folders.is_empty() {
        return Ok(Listed::Complete);
    }
    let next = AtomicUsize::new(0);
    let stop = AtomicBool::new(false);
    let (tx, rx) = std::sync::mpsc::sync_channel::<Result<ListPage, String>>(LIST_WORKERS * 2);
    std::thread::scope(|scope| {
        for _ in 0..LIST_WORKERS.min(folders.len()) {
            let tx = tx.clone();
            let (folders, next, stop) = (&folders, &next, &stop);
            scope.spawn(move || {
                let halted = || stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed);
                while let Some(folder) = folders.get(next.fetch_add(1, Ordering::Relaxed)) {
                    let mut token: Option<String> = None;
                    loop {
                        if halted() {
                            return;
                        }
                        let mut request =
                            ListRequest::recursive(folder).with_max_keys(SCAN_PAGE);
                        if let Some(token) = token.take() {
                            request = request.with_continuation(token);
                        }
                        let page = drive.list(&request).map_err(|e| e.to_string());
                        let more = page.as_ref().ok().and_then(|p| p.next.clone());
                        let failed = page.is_err();
                        // The receiver gone (the search stopped), or this listing failed.
                        if tx.send(page).is_err() || failed {
                            return;
                        }
                        match more {
                            Some(more) => token = Some(more),
                            None => break,
                        }
                    }
                }
            });
        }
        drop(tx);
        let mut ended = Ok(Listed::Complete);
        for page in rx.iter() {
            let page = match page {
                Ok(page) => page,
                Err(error) => {
                    ended = Err(error);
                    break;
                }
            };
            if !on_page(page) || cancel.load(Ordering::Relaxed) {
                ended = Ok(Listed::Stopped);
                break;
            }
        }
        stop.store(true, Ordering::Relaxed);
        // Dropped before the scope joins the workers: one blocked on a full channel then ends.
        drop(rx);
        // The workers end on a cancel without a word: the listing is not complete then.
        if matches!(ended, Ok(Listed::Complete)) && cancel.load(Ordering::Relaxed) {
            ended = Ok(Listed::Stopped);
        }
        ended
    })
}

/// Keeps a complete listing of `prefix` for the next search: a kept listing of a folder above it
/// keeps its other folders' objects (the fresh ones replace this folder's); else this one
/// replaces it. A failed write only costs the next search its head start.
fn keep_listing(
    file: &Path,
    cached: Option<find::CachedListing>,
    prefix: &str,
    objects: Vec<ObjectInfo>,
) {
    let at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs());
    let listing = match cached {
        Some(mut wider) if prefix.starts_with(wider.prefix.as_str()) && prefix != wider.prefix => {
            wider.objects.retain(|o| !o.key.starts_with(prefix));
            wider.objects.extend(objects);
            wider.objects.sort_by(|a, b| a.key.cmp(&b.key));
            wider.at = at;
            wider
        }
        _ => find::CachedListing {
            prefix: prefix.to_string(),
            at,
            objects,
        },
    };
    if listing.objects.len() <= find::CACHE_MAX_OBJECTS {
        if let Err(e) = find::write_listing(file, &listing) {
            eprintln!("[azdrive] the listing could not be kept in {}: {e}", file.display());
        }
    }
}

// ==== A drive's full-text index ====

/// A PDF's text through azul's reader (the PDF viewer's): every page's text blocks, a line
/// each, a blank line after a page; `None` for bytes that are no PDF.
pub(crate) fn pdf_text(bytes: &[u8]) -> Option<String> {
    let pdf = azul::pdf::ParsedPdf::create_from_bytes(U8VecRef::from(bytes));
    if !pdf.is_valid() {
        return None;
    }
    let mut text = String::new();
    for page in 0..pdf.page_count() {
        for block in pdf.page_text(page).as_slice() {
            text.push_str(block.as_str());
            text.push('\n');
        }
        text.push('\n');
        if text.len() >= azul_search_index::MAX_TEXT_BYTES {
            break;
        }
    }
    Some(text)
}

/// What AzDrive reads beyond plain text, office documents and mail: PDFs, through azul's reader.
pub(crate) fn extractors() -> Extractors {
    let pdf: ExtractFn = Arc::new(pdf_text);
    Extractors { pdf: Some(pdf) }
}

/// Brings the drive `drive_id`'s index in `dir` up to its folder `root` (the search box's
/// default walk, [`find::index_filters`]): how far it got now and then through `emit`; the
/// answer says what it did and what the index holds - or why it could not (another window
/// updating it, a folder that cannot be written).
pub(crate) fn run_index_update(
    drive_id: &str,
    root: &Path,
    dir: &Path,
    extractors: &Extractors,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let result = DriveIndex::open(dir).and_then(|index| {
        let held = index.status();
        let summary = index.update(
            root,
            &find::index_filters(),
            extractors,
            cancel,
            &mut |progress| {
                emit(Outcome::IndexProgress {
                    drive_id: drive_id.to_string(),
                    progress,
                    held,
                });
            },
        )?;
        Ok((summary, index.status()))
    });
    Outcome::Indexed {
        drive_id: drive_id.to_string(),
        result: result.map_err(|e| e.to_string()),
    }
}

/// Brings the drive `drive_id`'s index in `dir` up to the drive's files as `source` reads them
/// (its listing; each file from its local copy, or downloaded within the cap when allowed): how
/// far it got now and then through `emit`; the answer says what it did and what the index holds
/// - or why it could not.
pub(crate) fn run_drive_index_update(
    drive_id: &str,
    source: &DriveSource,
    dir: &Path,
    extractors: &Extractors,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    Outcome::Indexed {
        drive_id: drive_id.to_string(),
        result: drive_index_update(drive_id, source, dir, extractors, cancel, emit),
    }
}

/// [`run_drive_index_update`]'s work.
fn drive_index_update(
    drive_id: &str,
    source: &DriveSource,
    dir: &Path,
    extractors: &Extractors,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Result<(UpdateSummary, IndexStatus), String> {
    let index = DriveIndex::open(dir).map_err(|e| e.to_string())?;
    let held = index.status();
    // Every file of the drive, from its listing (an encrypted drive's: its drive index).
    let mut objects: Vec<ObjectInfo> = Vec::new();
    let everything = ListRequest::recursive("");
    let listed = list_pages(&*source.drive, &everything, cancel, &mut Vec::new(), &mut |page| {
        objects.extend(page.objects);
        true
    })?;
    if matches!(listed, Listed::Stopped) {
        let summary = UpdateSummary {
            listed: objects.len(),
            cancelled: true,
            ..UpdateSummary::default()
        };
        return Ok((summary, index.status()));
    }
    let files: Vec<azul_search::FileEntry> = objects
        .into_iter()
        .filter_map(|object| readable(drive_id, source, object))
        .collect();
    let mut read = |file: &azul_search::FileEntry, _kind: Kind, limit: u64| {
        read_from_source(drive_id, source, file, limit)
    };
    let summary = index
        .update_files(&files, &mut read, extractors, cancel, &mut |progress| {
            emit(Outcome::IndexProgress {
                drive_id: drive_id.to_string(),
                progress,
                held,
            });
        })
        .map_err(|e| e.to_string())?;
    Ok((summary, index.status()))
}

/// The file the index reads for `object` of a drive: its local copy's size and date when there
/// is one, else the listing's when it may be downloaded; `None` (not read): a folder, a hidden
/// item, a file with no copy here that may not be downloaded or is over the cap.
fn readable(
    drive_id: &str,
    source: &DriveSource,
    object: ObjectInfo,
) -> Option<azul_search::FileEntry> {
    if object.key.ends_with('/') || find::hidden_path(&object.key) {
        return None;
    }
    if let Some(copy) = source.sync.local_copy(drive_id, &object.key) {
        let meta = fs::metadata(copy).ok()?;
        return Some(azul_search::FileEntry {
            path: object.key,
            size: meta.len(),
            modified: modified_secs(&meta),
        });
    }
    source
        .download_cap
        .filter(|cap| object.size <= *cap)
        .map(|_| azul_search::FileEntry {
            path: object.key,
            size: object.size,
            modified: object.modified,
        })
}

/// The first `limit` bytes of a drive's file for its index: from its local copy, else downloaded
/// (within the cap; the bytes are dropped once read). `Err` when it cannot be read now.
fn read_from_source(
    drive_id: &str,
    source: &DriveSource,
    file: &azul_search::FileEntry,
    limit: u64,
) -> Result<Option<Vec<u8>>, String> {
    use std::io::Read as _;

    if let Some(copy) = source.sync.local_copy(drive_id, &file.path) {
        let mut bytes = Vec::new();
        let read = fs::File::open(&copy).and_then(|f| f.take(limit).read_to_end(&mut bytes));
        return match read {
            Ok(_) => Ok(Some(bytes)),
            Err(e) => Err(format!("{}: {e}", copy.display())),
        };
    }
    match source.download_cap {
        Some(cap) if file.size <= cap => {
            let bytes = if file.size > limit {
                source
                    .drive
                    .get_range(&file.path, ByteRange::new(0, Some(limit.saturating_sub(1))))
            } else {
                source.drive.get(&file.path)
            };
            bytes.map(Some).map_err(|e| e.to_string())
        }
        _ => Ok(None),
    }
}

/// When a file on this computer was last modified (seconds since 1970).
fn modified_secs(meta: &fs::Metadata) -> Option<u64> {
    meta.modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map(|d| d.as_secs())
}

/// Whether the file `rel` is read as text (a plain file, or one of no known kind: the walk's
/// reader passes over a binary one) rather than through its document's text.
fn read_as_text(rel: &str) -> bool {
    matches!(azul_search_index::kind_of(key_name(rel)), Some(Kind::Text) | None)
}

/// The last segment of a `/`-separated path.
fn key_name(rel: &str) -> &str {
    rel.rsplit('/').next().unwrap_or(rel)
}

/// The size and date of the file `rel` below `root` now; `None` when it is gone.
fn file_facts(root: &Path, rel: &str) -> Option<(u64, Option<u64>)> {
    let mut path = root.to_path_buf();
    for segment in rel.split('/').filter(|s| !s.is_empty()) {
        path.push(segment);
    }
    let meta = fs::metadata(path).ok()?;
    Some((meta.len(), modified_secs(&meta)))
}

/// The contents part of an indexed search as it goes: the results so far against
/// [`find::FIND_MAX`], the files a content result was handed over for.
struct IndexedContents<'a> {
    request: &'a azul_search::Request,
    prefix: &'a str,
    ask: &'a find::IndexAsk,
    matcher: &'a azul_search::ContentMatcher,
    extractors: &'a Extractors,
    cancel: &'a AtomicBool,
    found: usize,
    limited: bool,
    keys: HashSet<String>,
}

impl IndexedContents<'_> {
    fn full(&mut self) -> bool {
        if self.found >= find::FIND_MAX {
            self.limited = true;
        }
        self.limited || self.cancel.load(Ordering::Relaxed)
    }

    fn hand_over(
        &mut self,
        found: find::Found,
        batch: &mut FindBatch,
        emit: &mut dyn FnMut(Outcome),
    ) {
        if self.full() || !self.keys.insert(found.entry.key.clone()) {
            return;
        }
        self.found += 1;
        batch.push(found, emit);
    }

    /// Plain files (`rels` below the searched folder) read as the walk reads them: a matching
    /// one with its line.
    fn read_texts(
        &mut self,
        rels: &[String],
        batch: &mut FindBatch,
        emit: &mut dyn FnMut(Outcome),
    ) {
        if rels.is_empty() || self.full() {
            return;
        }
        let mut request = self.request.clone();
        request.limits.max_results = find::FIND_MAX.saturating_sub(self.found);
        let searched_before = batch.searched;
        let result = azul_search::search_listed(&request, rels, self.cancel, &mut |event| {
            match event {
                azul_search::Event::Content(hit) => {
                    let found = find::found_content(self.prefix, hit);
                    self.hand_over(found, batch, &mut *emit);
                }
                azul_search::Event::Progress(progress) => {
                    batch.searched = searched_before + progress.searched;
                    batch.tick(&mut *emit);
                }
                azul_search::Event::Phase(_) | azul_search::Event::Name(_) => {}
            }
        });
        if let Ok(summary) = result {
            batch.searched = searched_before + summary.searched;
            self.limited |= summary.limited;
        }
    }

    /// Documents (`rels` below the searched folder) read through their text: a matching one
    /// with its line; with `named` (the index named them) one whose words were apart too,
    /// without a line.
    fn read_documents(
        &mut self,
        rels: &[String],
        named: bool,
        batch: &mut FindBatch,
        emit: &mut dyn FnMut(Outcome),
    ) {
        let request = self.request;
        let refine = &request.filters.refine;
        for rel in rels {
            if self.full() {
                return;
            }
            let Some((size, modified)) = file_facts(&request.root, rel) else {
                continue; // gone since
            };
            if !refine.admits_name(key_name(rel), false)
                || !refine.admits_facts(Some(size), modified)
            {
                continue;
            }
            let path = format!("{}{rel}", self.ask.under);
            let text = azul_search_index::document_text(&self.ask.root, &path, self.extractors);
            batch.searched += 1;
            let line = text.and_then(|text| find::document_line(&text, self.matcher));
            if line.is_some() || named {
                let found = find::found_document(self.prefix, rel, size, modified, line);
                self.hand_over(found, batch, &mut *emit);
            }
            batch.tick(&mut *emit);
        }
    }
}

/// The search box's search of a folder on this computer whose drive has an index (`ask`): the
/// names as [`run_find`] finds them, then the contents - the files the index names first, at
/// once (a plain file read again for its line, so it matches as the walk's would; a document's
/// line from its text, or none when the index's words were apart), then the files the index
/// has not read as they are now (new or changed since its update: a plain file read as the walk
/// reads it, a document through its text) -, each once, at most [`find::FIND_MAX`]. An index
/// that cannot be opened or asked is passed by: the files are read as without one.
pub(crate) fn run_find_indexed(
    serial: u64,
    request: &azul_search::Request,
    prefix: &str,
    ask: &find::IndexAsk,
    extractors: &Extractors,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let Some(pattern) = request.contents.as_ref() else {
        return run_find(serial, request, prefix, cancel, emit);
    };
    let index = match DriveIndex::open(&ask.dir) {
        Ok(index) => index,
        Err(e) => {
            eprintln!("[azdrive] the index in {} is passed by: {e}", ask.dir.display());
            return run_find(serial, request, prefix, cancel, emit);
        }
    };
    let mut batch = FindBatch::new(serial);
    let matcher = match azul_search::ContentMatcher::new(pattern) {
        Ok(matcher) => matcher,
        Err(e) => {
            return batch.finish(FindEnd {
                error: Some(e.to_string()),
                ..FindEnd::default()
            })
        }
    };
    // The names, as without an index (a row found by its name gets its line below).
    let names = azul_search::Request {
        contents: None,
        ..request.clone()
    };
    let result = azul_search::search(&names, cancel, &mut |event| match event {
        azul_search::Event::Phase(_) => batch.phase(FindPhase::Names, &mut *emit),
        azul_search::Event::Name(hit) => batch.push(find::found_name(prefix, hit), &mut *emit),
        azul_search::Event::Content(_) => {}
        azul_search::Event::Progress(progress) => {
            batch.searched = progress.searched;
            batch.tick(&mut *emit);
        }
    });
    let summary = match result {
        Ok(summary) => summary,
        Err(e) => {
            return batch.finish(FindEnd {
                error: Some(e.to_string()),
                ..FindEnd::default()
            })
        }
    };
    if summary.limited || cancel.load(Ordering::Relaxed) {
        return batch.finish(FindEnd {
            limited: summary.limited,
            ..FindEnd::default()
        });
    }
    batch.phase(FindPhase::Contents, emit);
    let mut contents = IndexedContents {
        request,
        prefix,
        ask,
        matcher: &matcher,
        extractors,
        cancel,
        found: summary.names,
        limited: false,
        keys: HashSet::new(),
    };
    // The index's files below the folder, best first.
    let named = match index.query(&pattern.text, &ask.under, find::FIND_MAX) {
        Ok(paths) => Some(paths),
        Err(e) => {
            eprintln!("[azdrive] the index in {} is passed by: {e}", ask.dir.display());
            None
        }
    };
    let below = |path: &String| path.strip_prefix(ask.under.as_str()).map(str::to_string);
    if let Some(paths) = &named {
        let rels: Vec<String> = paths
            .iter()
            .filter_map(below)
            .filter(|rel| find::index_admits(&request.filters, rel))
            .collect();
        let (texts, documents): (Vec<String>, Vec<String>) =
            rels.into_iter().partition(|rel| read_as_text(rel));
        contents.read_texts(&texts, &mut batch, emit);
        contents.read_documents(&documents, true, &mut batch, emit);
    }
    // The files the index has not read as they are now (all of them without the index).
    let mut listed = Vec::new();
    if !contents.full() {
        let _ = azul_search::list_files(&request.root, &request.filters, cancel, &mut |file| {
            listed.push(azul_search::FileEntry {
                path: format!("{}{}", ask.under, file.path),
                ..file
            });
        });
    }
    let mut rest: Vec<String> = match named {
        Some(_) => index.unread(&listed).iter().filter_map(|f| below(&f.path)).collect(),
        None => listed.iter().filter_map(|f| below(&f.path)).collect(),
    };
    rest.retain(|rel| !contents.keys.contains(&format!("{prefix}{rel}")));
    rest.sort();
    let (texts, documents): (Vec<String>, Vec<String>) =
        rest.into_iter().partition(|rel| read_as_text(rel));
    contents.read_texts(&texts, &mut batch, emit);
    contents.read_documents(&documents, false, &mut batch, emit);
    let limited = contents.limited;
    batch.finish(FindEnd {
        limited,
        ..FindEnd::default()
    })
}

/// Seconds between two questions about a checkout's payment.
const PAYMENT_POLL_SECS: u64 = 3;
/// How long the dialog waits for a payment before it gives up (the payment page stays valid).
const PAYMENT_WAIT_SECS: u64 = 3600;

/// The most seconds between two questions about a payment while the token server does not
/// answer (or asks to wait): the wait doubles from [`PAYMENT_POLL_SECS`] up to this.
const PAYMENT_BACKOFF_MAX_SECS: u64 = 60;

/// The most seconds the token server's own `Retry-After` is obeyed for in one wait: it is heard
/// (a rate limit that says a minute is not asked again in three seconds), but ten minutes is the
/// most a dialog or a round waits for it.
const RETRY_AFTER_MAX_SECS: u64 = 600;

/// Seconds between two rounds of the background claims.
const CLAIM_POLL_SECS: u64 = 10;
/// The most seconds between two rounds of the claims while the token server does not answer.
const CLAIM_BACKOFF_MAX_SECS: u64 = 300;

/// The seconds to wait before asking again: `base` doubled for each of the `failures` questions
/// in a row that got no answer or a busy one, up to `cap`; never less than the token server asked
/// for (`retry_after`, heard up to [`RETRY_AFTER_MAX_SECS`]) - AZ-P10.
#[must_use]
pub(crate) fn backoff_secs(base: u64, failures: u32, cap: u64, retry_after: Option<u64>) -> u64 {
    let doubled = base
        .saturating_mul(1_u64 << failures.min(16))
        .min(cap.max(base));
    let asked = retry_after.map_or(0, |secs| secs.min(RETRY_AFTER_MAX_SECS));
    doubled.max(asked)
}
/// How long the background claims go on in one run (the next start asks again).
const CLAIM_WAIT_SECS: u64 = 3600;

/// A checkout of `tier`, its sign-up sealed to a claim key. The order's entry (the claim key's
/// secret, its tier, the drive's `name`, its token server) goes on the keyring's list BEFORE the
/// order is sent ([`pending::place_order`]): an answer that is lost leaves a record, the same
/// order asked again goes out under the same claim key and gets the first answer, and no second
/// order of the same intent is open at the same time - from this window, a second window or after
/// a restart (AZ-P4, AZ-P8). The checkout takes the entry's place ([`pending::posted`]) before
/// its payment page opens, so a drive paid after this window stopped waiting, or after AzDrive
/// closed, is claimed with it.
#[allow(clippy::too_many_arguments)]
fn start_checkout(
    token_url: &str,
    tier: &str,
    months: u32,
    method: &str,
    name: &str,
    keyring: &SharedKeyring,
    via: Option<&PayVia>,
    renew: Option<&RenewJob>,
    quote: Option<&Quote>,
) -> Result<Started, StartFailed> {
    let transport = AzulTransport::azlin(USER_AGENT);
    // The checkout's key is kept with the keyring's open calls: an order whose answer was lost
    // is asked again under it and gets its first answer.
    let server = TokenServer::new(token_url, &transport)
        .map_err(|e| StartFailed::from(token_error_text(&e)))?
        .with_outbox(keyring.outbox());
    // A renewal names the drive's price lock; without one (or with one the token server no
    // longer takes) it buys period tokens only, at today's price - the price the dialog shows.
    // A lock that could not be looked for (a vault unreadable, the drive not open) is not "no
    // lock": nothing is ordered at today's price then (X-DELTA-AZDRIVE-MONEY-6).
    let lock = match renew.filter(|r| !r.order.change_tier) {
        None => None,
        Some(r) => match renewal_lock_of(r) {
            Ok(lock) => lock,
            Err(why) => {
                return Err(StartFailed::from(Text::from(
                    Phrase::new("azdrive-order-lock-unread").arg("why", why),
                )))
            }
        },
    };
    order_and_start(
        &server, tier, months, method, name, keyring, via, renew, lock, quote,
    )
}

/// The order of `tier` for `months` placed at `server` and carried on to what the buyer is shown
/// ([`finish_start`]): a renewal under the price `lock` names it; when the token server no longer
/// takes the lock (used, not this checkout's, another tier, none, not paid) the order goes again
/// without it, as period tokens only at today's price - the quote still holds the charge to what
/// the buyer was shown. The first refusal ended that order's entry on the list, so the second is
/// a new one.
#[allow(clippy::too_many_arguments)]
fn order_and_start(
    server: &TokenServer<'_>,
    tier: &str,
    months: u32,
    method: &str,
    name: &str,
    keyring: &SharedKeyring,
    via: Option<&PayVia>,
    renew: Option<&RenewJob>,
    lock: Option<azcloud_kit::RenewalLock>,
    quote: Option<&Quote>,
) -> Result<Started, StartFailed> {
    let fresh = ClaimKey::generate()
        .map_err(|e| StartFailed::from(token_error_text(&TokenError::Config(e.to_string()))))?;
    let mut draft = PendingCheckout::posting(&fresh, tier, server.base(), name);
    if let Some(r) = renew {
        draft.for_drive = r.order.drive_id.clone();
        draft.change_tier = r.order.change_tier;
    }
    let order = |what: CheckoutFor<'_>| {
        pending::place_order(keyring, &draft, |claim| {
            checkout_for(server, claim, tier, months, method, via, what)
        })
    };
    let placed = match order(order_of(renew, lock.as_ref())) {
        Err(pending::OrderError::Token(e)) if lock.is_some() && e.is_lock_refused() => {
            // The refusal ended that order's entry: the one without the lock is a new one.
            order(order_of(renew, None)).map(|placed| (placed, None))
        }
        other => other.map(|placed| (placed, lock.as_ref().map(|l| l.checkout_id.clone()))),
    };
    let (placed, renews) = placed.map_err(|e| StartFailed::from(order_error_text(&e)))?;
    let (checkout, answer) = placed.answer;
    let mut kept = PendingCheckout::new(
        &checkout.checkout_id,
        &placed.claim,
        tier,
        server.base(),
        name,
    );
    kept.ladder_version = checkout.ladder_version;
    if let Some(r) = renew {
        kept.for_drive = r.order.drive_id.clone();
        kept.change_tier = r.order.change_tier;
        kept.renews = renews.unwrap_or_default();
    }
    finish_start(
        server,
        keyring,
        &placed.posting_id,
        tier,
        months,
        via,
        quote,
        checkout,
        answer,
        kept,
    )
}

/// The price lock of the drive a Renew order is for, when it has one - looked for in ALL the
/// drive's vaults ([`DriveTokens::state_of_all`]): a lock in a vault this computer could not read
/// is not "no lock". Why not, when the vaults were not all read.
fn renewal_lock_of(renew: &RenewJob) -> Result<Option<azcloud_kit::RenewalLock>, String> {
    let tokens = drive_tokens(&renew.store, &renew.order.drive_id, renew.azlin.as_ref())?;
    let kept = tokens.state_of_all().map_err(|e| e.to_string())?;
    Ok(kept.renewal_lock(&renew.order.tier).cloned())
}

/// An order that was not placed, in the app's words: the token server's error as its table words
/// it, or why the list did not begin the order.
fn order_error_text(e: &pending::OrderError) -> Text {
    match e {
        pending::OrderError::Token(e) => token_error_text(e),
        pending::OrderError::Refused(e) => add_error_text(e),
    }
}

/// Why the keyring's list did not take an order, in the app's words.
fn add_error_text(e: &pending::AddError) -> Text {
    match e {
        // The earlier order is paid, or may be: its drive comes; nothing new is ordered.
        pending::AddError::SameIntentOpen {
            checkout_id,
            may_be_paid: true,
        } => Phrase::new("azdrive-order-earlier-paid")
            .arg("checkout", checkout_id.as_str())
            .into(),
        pending::AddError::SameIntentOpen {
            checkout_id,
            may_be_paid: false,
        } => Phrase::new("azdrive-order-earlier-open")
            .arg("checkout", checkout_id.as_str())
            .into(),
        pending::AddError::NoRoom { waiting } => Phrase::new("azdrive-order-no-room")
            .arg("waiting", *waiting)
            .into(),
        pending::AddError::Keyring(e) => Phrase::new("azdrive-order-not-kept")
            .arg("why", e.to_string())
            .into(),
    }
}

/// What a checkout buys: a new drive, or for a Renew order the drive's next months under
/// `lock`, else period tokens only (an upgrade's sold only while the capacity has room).
fn order_of<'a>(
    renew: Option<&RenewJob>,
    lock: Option<&'a azcloud_kit::RenewalLock>,
) -> CheckoutFor<'a> {
    match (renew, lock) {
        (None, _) => CheckoutFor::NewDrive,
        (Some(r), Some(lock)) if !r.order.change_tier => CheckoutFor::Renewal(lock),
        (Some(r), _) => CheckoutFor::PeriodTokens {
            upgrade: r.order.upgrade,
        },
    }
}

/// The checkout of `tier` for `months` through `via` (else the v1 checkout by `method`) for
/// `what`, its sign-up sealed to `claim` (the claim key of the order's entry on the list).
fn checkout_for(
    server: &TokenServer<'_>,
    claim: &ClaimKey,
    tier: &str,
    months: u32,
    method: &str,
    via: Option<&PayVia>,
    what: CheckoutFor<'_>,
) -> Result<(Checkout, Option<serde_json::Value>), TokenError> {
    let (checkout, answer) = match via {
        // Cash by post: the method alone (the token server takes it with or without payment
        // options).
        Some(via) if via.surface == SurfaceKind::Paper => {
            let (checkout, answer) =
                server.checkout_cash(tier, months, &via.country, via.consent, claim, what)?;
            (checkout, Some(answer))
        }
        None => (server.checkout(tier, months, method, claim, what)?, None),
        Some(via) => {
            // Only the details the method's provider wants from the app, only for this request.
            let payer: Vec<(&str, &str)> = via
                .payer
                .as_ref()
                .map(|p| {
                    azul_pay::identity::APP_FIELDS
                        .iter()
                        .map(|f| (f.as_str(), p.get(*f)))
                        .filter(|(_, text)| !text.trim().is_empty())
                        .collect()
                })
                .unwrap_or_default();
            let through = CheckoutVia {
                provider: via.choice.provider.spec.id,
                method: via.choice.method.method.as_str(),
                surface: via.surface.as_str(),
                vat_country: &via.country,
                withdrawal_consent: via.consent,
                payer: &payer,
            };
            let (checkout, answer) = server.checkout_via(tier, months, &through, claim, what)?;
            (checkout, Some(answer))
        }
    };
    Ok((checkout, answer))
}

/// A checkout made: its entry takes the order's place on the keyring's list as `kept` before
/// anything shows it (cash by post with its slip), and then what is shown of it is checked:
/// the price against the buyer's `quote` ([`Quote::check`]), the v1 payment page against the one
/// the token server's address builds, the surface through azul-pay. Anything that is not as the
/// buyer was told ends the checkout at the token server ([`not_shown`]) and is said. `posting_id`
/// is the order's entry ([`pending::posted`]).
#[allow(clippy::too_many_arguments)]
fn finish_start(
    server: &TokenServer<'_>,
    keyring: &SharedKeyring,
    posting_id: &str,
    tier: &str,
    months: u32,
    via: Option<&PayVia>,
    quote: Option<&Quote>,
    checkout: Checkout,
    answer: Option<serde_json::Value>,
    mut kept: PendingCheckout,
) -> Result<Started, StartFailed> {
    let unshown = |why: StartFailed| not_shown(server, keyring, posting_id, &checkout, why);
    // Cash by post: the slip, checked, waits in the list with its checkout (it is printed
    // again from there); one that is not this order's is never printed. A new drive's order
    // keeps the drive's key there too, whose id the buyer's copy prints.
    let cash = match (via, &answer) {
        (Some(via), Some(answer)) if via.surface == SurfaceKind::Paper => {
            let slip = cash_slip(answer, via).map_err(|why| unshown(Text::plain(why).into()))?;
            crate::cash::keep_order(&mut kept, &slip, tier, months)
                .map_err(|why| unshown(crate::l10n::said(&why).into()))?;
            Some(slip)
        }
        _ => None,
    };
    if let Err(e) = pending::posted(keyring, posting_id, &kept) {
        return Err(unshown(add_error_text(&e).into()));
    }
    // The price the buyer was shown is the price charged, before anything is shown or opened.
    if let Some(changed) = quote.and_then(|quote| quote.check_checkout(&checkout).err()) {
        eprintln!(
            "[azdrive] the checkout {} is not the order shown: {changed}",
            checkout.checkout_id
        );
        // The same price for another tier or another number of months is another order.
        let why = match &changed.order {
            Some((shown, charged)) => Phrase::new("azdrive-order-changed")
                .arg("shown", shown.to_string())
                .arg("charged", charged.to_string()),
            None => Phrase::new("azdrive-order-price-changed")
                .arg("shown", crate::pay_words::show(&changed.shown))
                .arg("charged", crate::pay_words::show(&changed.charged)),
        };
        return Err(unshown(StartFailed {
            why: why.into(),
            prices_moved: true,
        }));
    }
    // The v1 payment page is the one this token server builds for the checkout; an address of
    // any other host, scheme or path is never opened (AZ-P3).
    if via.is_none()
        && !azcloud_kit::page_is(server.base(), &checkout.checkout_id, &checkout.pay_url)
    {
        eprintln!(
            "[azdrive] the payment page of the checkout {} is not this token server's",
            checkout.checkout_id
        );
        return Err(unshown(Text::key("azdrive-order-page-not-ours").into()));
    }
    let created = match (via, answer) {
        (Some(_), Some(_)) if cash.is_some() => None,
        (Some(via), Some(answer)) => match Created::parse(&answer, &via.choice, &via.look) {
            Ok(created) => Some(created),
            // Nothing of it is shown: nobody may pay it. It leaves the list only when the
            // token server ended it - and the words say when it did not.
            Err(refused) => return Err(unshown(Text::plain(refused.to_string()).into())),
        },
        _ => None,
    };
    Ok(Started {
        checkout,
        kept,
        created,
        cash,
    })
}

/// The checkout `checkout` was made and is not shown (`why`): nobody may pay it, so the token
/// server is asked to end it. Whether it did decides what the buyer is told (X-AR5b-8): an order
/// that could not be ended stays on the keyring's list, in flight, in case it is paid.
fn not_shown(
    server: &TokenServer<'_>,
    keyring: &SharedKeyring,
    posting_id: &str,
    checkout: &Checkout,
    mut why: StartFailed,
) -> StartFailed {
    match pending::abandon_unshown(server, keyring, posting_id, &checkout.checkout_id) {
        Abandoned::Ended | Abandoned::Settled => why,
        Abandoned::Kept { paid, why: kept } => {
            eprintln!(
                "[azdrive] the checkout {} was not shown and stays on the list: {kept}",
                checkout.checkout_id
            );
            why.why = why.why.then(" ").then(
                Phrase::new(if paid {
                    "azdrive-order-not-shown-paid"
                } else {
                    "azdrive-order-not-shown-open"
                })
                .arg("checkout", checkout.checkout_id.as_str()),
            );
            why
        }
    }
}

/// A cash checkout's answer as its slip: checked by azul-pay (its parts) and by azcloud-kit
/// (its activation code is this checkout's, for its amount and currency).
fn cash_slip(answer: &serde_json::Value, via: &PayVia) -> Result<CashSlip, String> {
    let slip = CashSlip::parse(answer, &via.choice).map_err(|e| e.to_string())?;
    ActivationCode::parse(&slip.activation_code)
        .and_then(|code| code.check(&slip.checkout_id, &slip.amount))
        .map_err(|e| {
            format!("the slip's activation code is not this order's ({e}), so it was not printed")
        })?;
    Ok(slip)
}

/// A claim code picked up (cash by post, on another computer): its checkout onto the keyring's
/// list - unless the list has it already (this computer bought it: its slip stays).
fn pick_up(keyring: &SharedKeyring, checkout: &PendingCheckout) -> Result<(), String> {
    let listed = pending::list(keyring).map_err(|e| e.to_string())?;
    if listed.iter().any(|c| c.checkout_id == checkout.checkout_id) {
        return Ok(());
    }
    pending::add(keyring, checkout).map_err(|e| e.to_string())
}

/// The checkout `checkout_id` on the surface `kind`, checked for `choice`. A refusal is said in
/// the table's words (a method without that surface says so, a busy token server when to try
/// again), never as the server's sentence.
fn switch_surface(
    token_url: &str,
    checkout_id: &str,
    kind: SurfaceKind,
    choice: &Choice,
    look: &Look,
) -> Result<azul_pay::Surface, Text> {
    let transport = AzulTransport::azlin(USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| token_error_text(&e))?;
    let answer = server
        .checkout_surface(checkout_id, kind.as_str())
        .map_err(|e| token_error_text(&e))?;
    let surface = answer
        .get("surface")
        .ok_or_else(|| Text::key("azdrive-pay-err-no-surface"))?;
    azul_pay::Surface::parse(surface, choice, look).map_err(|e| Text::plain(e.to_string()))
}

/// The checkout `checkout_id` abandoned at the token server: off the keyring's list only when
/// its answer ended it ([`pending::abandon`]).
fn abandon(token_url: &str, checkout_id: &str, keyring: &SharedKeyring) -> Abandoned {
    let transport = AzulTransport::azlin(USER_AGENT);
    match TokenServer::new(token_url, &transport) {
        Ok(server) => {
            // The abandon's key is kept with the keyring's open calls, as the checkout's is.
            let server = server.with_outbox(keyring.outbox());
            pending::abandon(&server, keyring, checkout_id)
        }
        Err(e) => Abandoned::Kept {
            paid: false,
            why: e.to_string(),
        },
    }
}

/// Asks the token server about `checkout` every few seconds until its drive is the app's (its
/// session in the keyring), the checkout ends (declined, gone: off the keyring's list), `cancel`
/// is set ("Stop waiting") or the wait is too long. A question without an answer is asked again
/// (the network may come back); the checkout stays on the list for the background claims and
/// the next start.
fn await_payment(
    serial: u64,
    checkout: &PendingCheckout,
    token_url: &str,
    keyring: &SharedKeyring,
    cancel: &AtomicBool,
) -> Outcome {
    let transport = AzulTransport::azlin(USER_AGENT);
    let server = match TokenServer::new(token_url, &transport) {
        Ok(server) => server,
        Err(e) => {
            return Outcome::PaymentEnded {
                serial,
                why: Text::plain(e.to_string()),
                ended: false,
            }
        }
    };
    let started = Instant::now();
    let mut last_problem = String::new();
    // Seconds before the next question: three while all is well, longer while the token server
    // does not answer or asks to wait (AZ-P10).
    let mut wait = PAYMENT_POLL_SECS;
    let mut failures = 0_u32;
    loop {
        // A few seconds in short steps: Stop waiting is heard at once.
        for _ in 0..(wait * 4) {
            if cancel.load(Ordering::SeqCst) {
                // "Stop waiting" said why, and the background claims take over.
                return Outcome::PaymentEnded {
                    serial,
                    why: Text::default(),
                    ended: false,
                };
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if started.elapsed() > Duration::from_secs(PAYMENT_WAIT_SECS) {
            let why = if last_problem.is_empty() {
                Text::key("azdrive-pay-no-payment")
            } else {
                Phrase::new("azdrive-pay-no-payment-last")
                    .arg("last", last_problem.as_str())
                    .into()
            };
            // No payment yet: the checkout stays in flight (its page is still valid).
            return Outcome::PaymentEnded {
                serial,
                why,
                ended: false,
            };
        }
        match pending::poll(&server, keyring, checkout) {
            Polled::Pending => {
                failures = 0;
                wait = PAYMENT_POLL_SECS;
            }
            Polled::Transient(why) => {
                last_problem = why.to_string();
                failures = failures.saturating_add(1);
                wait = backoff_secs(
                    PAYMENT_POLL_SECS,
                    failures,
                    PAYMENT_BACKOFF_MAX_SECS,
                    why.retry_after(),
                );
            }
            Polled::Final(Final::Claimed(claimed)) => {
                return Outcome::Claimed {
                    serial: Some(serial),
                    checkout: checkout.clone(),
                    claimed,
                }
            }
            Polled::Final(Final::Renewed {
                drive_id,
                months,
                record,
            }) => {
                return Outcome::Renewed {
                    serial: Some(serial),
                    checkout_id: checkout.checkout_id.clone(),
                    drive_id,
                    months,
                    record,
                }
            }
            Polled::Final(end) => {
                return Outcome::PaymentEnded {
                    serial,
                    why: Phrase::new("azdrive-pay-checkout-ended")
                        .arg("why", end.why())
                        .into(),
                    ended: true,
                }
            }
            Polled::Settled => {
                return Outcome::PaymentEnded {
                    serial,
                    why: Text::key("azdrive-pay-other-window"),
                    ended: true,
                }
            }
        }
    }
}

/// The background claims: the unfinished checkouts on the keyring's list, each asked at its
/// own token server (else `token_url`), every few seconds until none is left or an hour is gone
/// (the next start asks again). The list is read anew every round (another window adds to it).
/// A paid one's drive and a dropped one go to the window at once; a drive claimed here is not
/// reported again while the window finishes its checkout. A checkout whose drive is saved and
/// whose period tokens are not issued yet (AZLINSEC17 F24) is not asked about again: its tokens
/// are issued with the issue key it keeps ([`pending::finish`]) - the sealed sign-up may be
/// gone by now.
/// Whether a cash checkout last asked at `last` (in this claims run) is to be asked at `now`:
/// never asked yet, or `every` seconds (its daily look's interval) gone since.
#[must_use]
pub(crate) fn cash_ask_due(last: Option<Instant>, every: u64, now: Instant) -> bool {
    last.is_none_or(|at| now.saturating_duration_since(at) >= Duration::from_secs(every))
}

fn claim_pending(
    keyring: &SharedKeyring,
    token_url: Option<&str>,
    store: &PeriodTokenStore,
    drives: &HashMap<String, Arc<azcloud_kit::AzlinDrive>>,
    cash_every: u64,
    sender: &mut ThreadSender,
) -> Outcome {
    let transport = AzulTransport::azlin(USER_AGENT);
    let started = Instant::now();
    let mut reported: Vec<String> = Vec::new();
    // A cash checkout is asked once, then once each daily interval (its letter takes days):
    // never every round.
    let mut asked_cash: HashMap<String, Instant> = HashMap::new();
    let mut said_in_flight: Vec<String> = Vec::new();
    // Rounds in a row in which the token server did not answer or asked to wait, and the longest
    // wait it asked for in the round (AZ-P10).
    let mut failures = 0_u32;
    loop {
        let mut unanswered = false;
        let mut asked_for: Option<u64> = None;
        // The entry of an order whose answer never came is a record, not a checkout to ask
        // about; after a day nobody can pay what it may have made, and it goes.
        if let Err(e) = pending::forget_stale_postings(keyring, azcloud_kit::now()) {
            eprintln!("[azdrive] the records of orders without an answer were not cleared: {e}");
        }
        let open: Vec<PendingCheckout> = match pending::list(keyring) {
            Ok(checkouts) => checkouts
                .into_iter()
                .filter(|c| !c.is_posting())
                .filter(|c| c.period.is_some() || !reported.contains(&c.checkout_id))
                .filter(|c| {
                    !(c.is_cash()
                        && c.period.is_none()
                        && !cash_ask_due(
                            asked_cash.get(&c.checkout_id).copied(),
                            cash_every,
                            Instant::now(),
                        ))
                })
                .collect(),
            Err(e) => {
                // A system without a keyring kept no checkout either: nothing to say then.
                let no_keyring = matches!(
                    e.root(),
                    CloudError::Keyring(azul_storage::keyring::KeyringError::Unavailable)
                );
                return Outcome::ClaimsDone {
                    problem: (!no_keyring).then(|| {
                        Phrase::new("azdrive-pay-checkouts-unread")
                            .arg("why", e.to_string())
                            .into()
                    }),
                };
            }
        };
        if open.is_empty() {
            return Outcome::ClaimsDone { problem: None };
        }
        let mut asked = 0;
        for checkout in open {
            let url = if checkout.token_url.is_empty() {
                token_url.unwrap_or_default().to_string()
            } else {
                checkout.token_url.clone()
            };
            let Ok(server) = TokenServer::new(&url, &transport) else {
                continue;
            };
            asked += 1;
            if let Some(owed) = &checkout.period {
                let tokens = match drive_tokens(store, &owed.drive_id, drives.get(&owed.drive_id)) {
                    Ok(tokens) => tokens,
                    // tried again next round, quietly
                    Err(_) => continue,
                };
                let now = crate::actions::now_secs();
                let finished = pending::finish(&server, keyring, store, &tokens, &checkout, owed, now);
                // A try that failed, and a grant that waits for the drive's vault with nothing
                // new to say, is tried again next round, quietly.
                if !matches!(
                    finished,
                    Finished::Kept(_) | Finished::Waits { count: 0, .. }
                ) {
                    send(
                        sender,
                        Outcome::CheckoutFinished {
                            checkout_id: checkout.checkout_id.clone(),
                            result: Ok(Some(finished)),
                            from_claims: true,
                        },
                    );
                }
                continue;
            }
            if checkout.is_cash() {
                asked_cash.insert(checkout.checkout_id.clone(), Instant::now());
            }
            // Bought here and not paid yet: a new order in this window asks about it first.
            let may_be_in_flight = !checkout.is_cash() && !checkout.picked_up;
            let polled = pending::poll(&server, keyring, &checkout);
            if let Polled::Transient(why) = &polled {
                unanswered |= matches!(
                    why,
                    pending::Transient::Busy { .. } | pending::Transient::NoAnswer(_)
                );
                asked_for = asked_for.max(why.retry_after());
            }
            match polled {
                Polled::Pending if checkout.is_cash() => {
                    send(sender, Outcome::CashWaiting { checkout });
                }
                Polled::Pending | Polled::Transient(_)
                    if may_be_in_flight && !said_in_flight.contains(&checkout.checkout_id) =>
                {
                    said_in_flight.push(checkout.checkout_id.clone());
                    send(sender, Outcome::InFlight { checkout });
                }
                Polled::Final(Final::Claimed(claimed)) => {
                    reported.push(checkout.checkout_id.clone());
                    send(
                        sender,
                        Outcome::Claimed {
                            serial: None,
                            checkout,
                            claimed,
                        },
                    );
                }
                // Its months wait with it for the drive: the next round finishes it.
                Polled::Final(Final::Renewed {
                    drive_id,
                    months,
                    record,
                }) => send(
                    sender,
                    Outcome::Renewed {
                        serial: None,
                        checkout_id: checkout.checkout_id.clone(),
                        drive_id,
                        months,
                        record,
                    },
                ),
                Polled::Final(end) => send(
                    sender,
                    Outcome::CheckoutDropped {
                        checkout_id: checkout.checkout_id,
                        why: end.why(),
                    },
                ),
                Polled::Pending | Polled::Transient(_) | Polled::Settled => {}
            }
        }
        if asked == 0 {
            return Outcome::ClaimsDone {
                problem: Some(Text::key("azdrive-pay-claims-no-server")),
            };
        }
        if started.elapsed() > Duration::from_secs(CLAIM_WAIT_SECS) {
            return Outcome::ClaimsDone { problem: None };
        }
        failures = if unanswered {
            failures.saturating_add(1)
        } else {
            0
        };
        let wait = backoff_secs(CLAIM_POLL_SECS, failures, CLAIM_BACKOFF_MAX_SECS, asked_for);
        std::thread::sleep(Duration::from_secs(wait));
    }
}

/// A drive's period tokens: kept with the drive when its key is on this computer (every device
/// of the drive redeems them and issues its next months), else on this computer only - and then
/// the tokens say why ([`azcloud_kit::NoVault`]): a paid grant is erased from the keyring only
/// when it is in two places ([`pending::finish`], X-AR5b-7), so whether the drive happened to be
/// open decides nothing about a paid month.
fn drive_tokens(
    store: &PeriodTokenStore,
    drive_id: &str,
    azlin: Option<&Arc<azcloud_kit::AzlinDrive>>,
) -> Result<DriveTokens, String> {
    use azcloud_kit::NoVault;
    #[cfg(feature = "encryption")]
    let why = match azlin {
        Some(azlin) => {
            let bucket: Arc<dyn Drive> = azlin.clone();
            let keyring =
                azul_storage::azul_keyring::azlin_keyring(azul_storage::keyring::Asking::User);
            match azcloud_kit::UnlockedDrive::open(drive_id, bucket, &keyring) {
                Ok(Some(unlocked)) => return store.for_drive(&unlocked).map_err(|e| e.to_string()),
                // a plain drive, or its key not here yet
                Ok(None) => NoVault::NoKey,
                Err(e) => NoVault::NotOpened(e.to_string()),
            }
        }
        None => NoVault::NotOpened(String::from("the drive is not open")),
    };
    #[cfg(not(feature = "encryption"))]
    let why = {
        let _ = azlin;
        NoVault::NoKey
    };
    store
        .on_this_device_because(drive_id, why)
        .map_err(|e| e.to_string())
}

/// A look at each of `drives` (its id, token server and the drive when it is open): its status
/// - the period, a pending recovery-key lockdown, a ban - and, when its period nears its end,
/// its next month from a period token kept in its vaults, or its grant's next month
/// ([`look_at_drive`]: under the drive's keyring lock, with its newest drive token).
fn redeem_periods(
    keyring: &SharedKeyring,
    store: &PeriodTokenStore,
    drives: &[(String, String, Option<Arc<azcloud_kit::AzlinDrive>>)],
) -> Outcome {
    let transport = AzulTransport::azlin(USER_AGENT);
    let now = crate::actions::now_secs();
    let mut results = Vec::new();
    let mut renewals = Vec::new();
    let mut news = None;
    for (drive_id, token_url, azlin) in drives {
        let Ok(server) = TokenServer::new(token_url, &transport) else {
            continue;
        };
        let Ok(tokens) = drive_tokens(store, drive_id, azlin.as_ref()) else {
            continue;
        };
        let look = look_at_drive(&server, keyring, store, &tokens, now);
        renewals.push((
            drive_id.clone(),
            renewal_seen(&server, keyring, store, &tokens, &look, azlin.as_ref(), now),
        ));
        // The tier list once a look (its news: a full drive's offer, the price notices).
        if news.is_none() {
            news = server.tier_news().ok();
        }
        results.push((drive_id.clone(), look));
    }
    Outcome::PeriodsRedeemed {
        results,
        renewals,
        news,
    }
}

/// What a look at `tokens`' drive saw of its next months: a change of tier bought for it
/// (taken when the token server allows: its credentials follow at once), the reminder due
/// (said once as a notification), a running monthly plan, its price lock.
#[allow(clippy::too_many_arguments)]
fn renewal_seen(
    server: &TokenServer<'_>,
    keyring: &SharedKeyring,
    store: &PeriodTokenStore,
    tokens: &DriveTokens,
    look: &azcloud_kit::Look,
    azlin: Option<&Arc<azcloud_kit::AzlinDrive>>,
    now: u64,
) -> crate::renewals::RenewalSeen {
    use crate::renewals::{ChangeSeen, RenewalSeen};
    // a smaller tier is judged on the node's signed count of the drive's bytes, asked from the
    // open drive only while a change is open
    let usage = || azlin.and_then(|drive| drive.usage_proof().ok().flatten());
    let change = match change_due(server, keyring, tokens, usage, now) {
        TierChanged::Changed {
            tier,
            credited_days,
            grant,
            ..
        } => {
            if let Some(azlin) = azlin {
                azlin.apply(grant);
            }
            Some(ChangeSeen::Changed { tier, credited_days })
        }
        TierChanged::Waits { from } => Some(ChangeSeen::Waits { from }),
        TierChanged::DoesNotFit { stored, quota } => Some(ChangeSeen::DoesNotFit { stored, quota }),
        TierChanged::Kept(why) => {
            eprintln!("[azdrive] the change of tier of {} waits: {why}", tokens.drive_id());
            None
        }
        TierChanged::Nothing | TierChanged::Banned => None,
    };
    // Whether the drive has a price lock is said only when every vault was read; what could be
    // read still shows the reminder and the plan.
    let (kept, lock_unread) = match tokens.state_of_all() {
        Ok(kept) => (kept, None),
        Err(e) => (tokens.state().unwrap_or_default(), Some(e.to_string())),
    };
    let status = look.status.as_ref();
    let reminder = status.and_then(|s| reminder_due(s, &kept, now));
    let notify = reminder
        .as_ref()
        .is_some_and(|r| store.remind_once(tokens.drive_id(), r).unwrap_or(false));
    let tier = status.and_then(|s| s.tier.clone());
    let plan = kept
        .grants
        .iter()
        .find(|g| g.recurring && g.is_open())
        .map(|g| g.checkout_id.clone());
    let locked = tier.as_deref().and_then(|t| kept.renewal_lock(t));
    let lock = locked.map(|l| (l.tier.clone(), l.ladder_version));
    let lock_paid = locked.and_then(|l| lock_paid(keyring, &l.checkout_id));
    RenewalSeen {
        quota_bytes: status.and_then(|s| s.quota_bytes),
        tier,
        reminder,
        notify,
        change,
        plan,
        lock,
        lock_paid,
        lock_unread,
    }
}

/// What this computer paid for the price lock (the paid checkout) `checkout_id`: its months and
/// what the token server booked for them; `None` when the paid list has no record of it (another
/// computer bought it, a token server that books no sale) - the renewal dialog then says no price
/// rather than today's list price (X-AR5b-20).
fn lock_paid(keyring: &SharedKeyring, checkout_id: &str) -> Option<crate::renewals::LockPaid> {
    // collapse-ok: a paid list that cannot be read shows no price; the order is still held to what the token server charges
    let record = keyring
        .paid()
        .and_then(|list| list.of_checkout(checkout_id))
        .ok()
        .flatten()?;
    Some(crate::renewals::LockPaid {
        months: record.months,
        price: record.charge()?,
    })
}

/// Ends the monthly plan `checkout_id` of `drive_id` with the issue key its vaults keep.
fn cancel_plan(
    token_url: &str,
    store: &PeriodTokenStore,
    azlin: Option<&Arc<azcloud_kit::AzlinDrive>>,
    drive_id: &str,
    checkout_id: &str,
) -> Result<(), String> {
    let transport = AzulTransport::azlin(USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let tokens = drive_tokens(store, drive_id, azlin)?;
    let state = tokens.state().map_err(|e| e.to_string())?;
    let grant = state
        .grant(checkout_id)
        .ok_or_else(|| format!("the drive keeps no plan {checkout_id}"))?;
    server
        .cancel_plan(checkout_id, &grant.issue_key)
        .map_err(|e| e.to_string())
}

/// The claimed `checkout`'s drive `drive_id` is in the drives file (AZDRIVE-INTEGRATION §4):
/// without a `grant` its checkout leaves the keyring's list; with one the list keeps its issue
/// key with its claim secret ([`pending::claimed`]) until the grant is kept with the drive and
/// its first month asked for at `token_url` ([`pending::finish`]).
fn finish_checkout(
    keyring: &SharedKeyring,
    store: &PeriodTokenStore,
    azlin: Option<&Arc<azcloud_kit::AzlinDrive>>,
    checkout: &PendingCheckout,
    drive_id: &str,
    grant: Option<&PeriodTokens>,
    token_url: &str,
) -> Outcome {
    let checkout_id = checkout.checkout_id.clone();
    let outcome = |result| Outcome::CheckoutFinished {
        checkout_id: checkout_id.clone(),
        result,
        from_claims: false,
    };
    if let Err(e) = pending::claimed(keyring, &checkout.checkout_id, drive_id, grant) {
        return outcome(Err(e.to_string()));
    }
    let Some(grant) = grant else {
        return outcome(Ok(None));
    };
    // What the issue needs; the moments of its wait for the drive's vault are the list's
    // ([`pending::claimed`] stamped them, [`pending::finish`] reads them there).
    let owed = PendingTokens::of(grant, drive_id, 0);
    let transport = AzulTransport::azlin(USER_AGENT);
    let finished = match (TokenServer::new(token_url, &transport), drive_tokens(store, drive_id, azlin)) {
        (Ok(server), Ok(tokens)) => {
            let now = crate::actions::now_secs();
            pending::finish(&server, keyring, store, &tokens, checkout, &owed, now)
        }
        (Err(e), _) => Finished::Kept(e.to_string()),
        (_, Err(why)) => Finished::Kept(why),
    };
    outcome(Ok(Some(finished)))
}

fn run_job(job: Job, sender: &mut ThreadSender) -> Outcome {
    match job {
        Job::Scan {
            drive,
            dir,
            prefix,
            refresh,
            serial,
            cancel,
        } => {
            let mut emit = |outcome: Outcome| send(sender, outcome);
            match dir {
                Some(dir) => scan_dir(&dir, &prefix, serial, &cancel, &mut emit),
                None => scan_bucket(&*drive, &prefix, refresh, serial, &cancel, &mut emit),
            }
        }
        Job::Stat { root, keys, serial } => Outcome::Stats {
            serial,
            stats: stats_of(&root, &keys),
        },
        Job::Count {
            drive_id,
            root,
            keys,
            show_hidden,
        } => Outcome::Counted {
            counts: keys
                .into_iter()
                .filter_map(|key| count_items(&root, &key, show_hidden).map(|n| (key, n)))
                .collect(),
            drive_id,
        },
        Job::Folders { drive, dir, node } => {
            let result = tree_folders(&*drive, dir.as_deref(), &node);
            let leaves = match (&result, dir.as_deref()) {
                (Ok((folders, _)), Some(dir)) if folders.len() <= LEAF_CHECKS => folders
                    .iter()
                    .filter(|f| !has_subfolder(&dir.join(azul_storage::key::last_segment(f))))
                    .cloned()
                    .collect(),
                _ => Vec::new(),
            };
            Outcome::Folders {
                result,
                node,
                leaves,
            }
        }
        Job::Plan {
            id,
            source,
            items,
            target,
            target_prefix,
            kind,
            copy,
        } => Outcome::Planned {
            id,
            result: fileops::plan_transfer(
                &*source,
                &items,
                &*target,
                &target_prefix,
                kind,
                &copy,
            ),
        },
        Job::Run {
            id,
            plan,
            source,
            target,
            kind,
            cancel,
        } => {
            // At most ten progress messages a second.
            let mut last = Instant::now();
            let report = fileops::run_transfer(&plan, &*source, &*target, kind, &cancel, &mut |p| {
                if last.elapsed().as_millis() >= 100 {
                    last = Instant::now();
                    send(
                        sender,
                        Outcome::Progress {
                            id,
                            progress: p.clone(),
                        },
                    );
                }
            });
            let _ = cancel.load(Ordering::SeqCst);
            Outcome::Ran { id, report }
        }
        Job::Delete {
            drive,
            drive_id,
            items,
            stamp,
        } => Outcome::Deleted {
            drive_id,
            deleted: fileops::delete_items(&*drive, &items, stamp.as_deref()),
        },
        Job::RestoreTrash {
            drive,
            drive_id,
            keys,
        } => Outcome::Restored {
            drive_id,
            result: fileops::restore_pairs(&*drive, &keys).and_then(|pairs| {
                fileops::restore_items(&*drive, &pairs)?;
                Ok(pairs.len())
            }),
        },
        Job::PurgeTrash {
            drive_id,
            root,
            marks_file,
            now,
        } => {
            let drive = LocalDrive::without_manifest(root.clone());
            Outcome::TrashPurged {
                drive_id,
                result: crate::trash::purge(&drive, &root, &marks_file, now),
            }
        }
        Job::Rename {
            drive,
            drive_id,
            from,
            to,
        } => Outcome::Renamed {
            result: drive.rename(&from, &to),
            drive_id,
            from,
            to,
        },
        Job::Create {
            drive,
            drive_id,
            key,
        } => Outcome::Created {
            result: if key.ends_with('/') {
                drive.create_folder(&key)
            } else {
                create_empty_file(&*drive, &key)
            },
            drive_id,
            key,
        },
        Job::Undo {
            drive,
            pairs,
            remove,
            trashed,
        } => {
            let result = if trashed {
                // (key, trash key) pairs: back where they were.
                fileops::restore_items(&*drive, &pairs)
            } else {
                // Each put back on its own: one that cannot be does not stop the others.
                let mut first_error = None;
                for (now, before) in &pairs {
                    if let Err(e) = drive.rename(now, before) {
                        first_error.get_or_insert(e);
                    }
                }
                match (first_error, &remove) {
                    (Some(e), _) => Err(e),
                    (None, Some(key)) => undo_create(&*drive, key),
                    (None, None) => Ok(()),
                }
            };
            Outcome::Undone { result }
        }
        Job::Preview {
            drive,
            key,
            size,
            kind,
            copies,
        } => Outcome::Previewed {
            content: make_preview(&*drive, &key, size, kind, &copies),
            key,
        },
        Job::Measure {
            drive,
            prefix,
            serial,
        } => Outcome::Measured {
            serial,
            result: measure(&*drive, &prefix),
        },
        Job::Metadata { drive, key } => Outcome::Metadata {
            result: drive.metadata(&key),
            key,
        },
        Job::Zip {
            drive,
            items,
            zip_key,
        } => {
            // The key it went to: the name the window chose, or the next free one.
            let made = make_zip(&*drive, &items, &zip_key);
            Outcome::Zipped {
                zip_key: made.as_ref().map_or(zip_key, |(_, key)| key.clone()),
                result: made.map(|(len, _)| len),
            }
        }
        Job::Open { drive, key, copies } => {
            let (result, kept_edit) = match drive.local_path(&key) {
                // A local file opens where it is.
                Some(path) => (Ok(path), None),
                None => match opened_copy_keeping_edits(&*drive, &copies, &key) {
                    Ok(opened) => (Ok(opened.path), opened.kept_edit),
                    Err(e) => (Err(e), None),
                },
            };
            Outcome::Opened {
                key,
                result,
                kept_edit,
            }
        }
        Job::Test {
            serial,
            entry,
            secret,
        } => {
            // ONE listing call, at most one entry: does the source answer with these settings?
            let result = entry
                .open_with_secret(
                    secret.as_deref(),
                    Box::new(AzulTransport::foreign(USER_AGENT)),
                )
                .and_then(|drive| drive.list(&ListRequest::folder("").with_max_keys(1)))
                .map(|page| {
                    if page.folders.is_empty() && page.objects.is_empty() {
                        String::from("azdrive-add-connection-ok-empty")
                    } else {
                        String::from("azdrive-add-connection-ok")
                    }
                });
            Outcome::Tested { serial, result }
        }
        Job::OAuthExchange {
            serial,
            plan,
            code,
            code_verifier,
            redirect_uri,
        } => {
            let result = azul_storage::oauth::exchange_code(
                &AzulTransport::foreign(USER_AGENT),
                &plan.token_url,
                &plan.client,
                &code,
                &code_verifier,
                &redirect_uri,
            )
            .map_err(|e| e.to_string());
            Outcome::SignedIn { serial, result }
        }
        Job::Tiers {
            serial,
            token_url,
            country,
        } => {
            let transport = AzulTransport::azlin(USER_AGENT);
            let news = TokenServer::new(&token_url, &transport)
                .and_then(|server| server.tier_news_in(&country))
                .map_err(|e| match buyer_refused(&e) {
                    Some(said) => TiersFailed::Refused(said),
                    None => TiersFailed::Failed(e.to_string()),
                });
            let closed = news.as_ref().map(|n| n.closed.clone()).unwrap_or_default();
            Outcome::Tiers {
                serial,
                country,
                result: news.map(|n| n.tiers),
                closed,
            }
        }
        Job::CreateTestDrive {
            serial,
            token_url,
            name,
            tier,
            keyring,
        } => {
            let transport = AzulTransport::azlin(USER_AGENT);
            let result = TokenServer::new(&token_url, &transport)
                .and_then(|server| server.create_dev_drive(&name, &tier))
                .map_err(|e| {
                    if e.is_checkout_only() {
                        Text::key("azdrive-add-checkout-only")
                    } else {
                        token_error_text(&e)
                    }
                })
                // The session into the keyring under the drive's lock before anything uses it,
                // as every later refresh of the drive writes it.
                .map(|bundle| match keyring.keep_new_drive(&bundle) {
                    Ok((session, _)) => BoughtDrive {
                        bundle,
                        session,
                        unsaved: None,
                    },
                    Err(e) => BoughtDrive {
                        session: bundle.session().to_keyring_secret(),
                        bundle,
                        unsaved: Some(e.to_string()),
                    },
                });
            Outcome::Bought { serial, result }
        }
        Job::Checkout {
            serial,
            token_url,
            tier,
            months,
            method,
            name,
            keyring,
            via,
            renew,
            quote,
        } => Outcome::CheckoutStarted {
            serial,
            result: start_checkout(
                &token_url,
                &tier,
                months,
                &method,
                &name,
                &keyring,
                via.as_deref(),
                renew.as_deref(),
                quote.as_ref(),
            ),
        },
        Job::CancelPlan {
            token_url,
            store,
            azlin,
            drive_id,
            checkout_id,
        } => Outcome::PlanCancelled {
            result: cancel_plan(&token_url, &store, azlin.as_ref(), &drive_id, &checkout_id),
            drive_id,
        },
        Job::SaveNotice { azlin, key, text } => Outcome::NoticeSaved {
            result: azlin.put(&key, text.as_bytes()),
            key,
        },
        Job::Options {
            serial,
            asked,
            token_url,
            tier,
            months,
            country,
            currency,
        } => {
            let transport = AzulTransport::azlin(USER_AGENT);
            let surfaces: Vec<&str> = crate::add_drive::PAY_SURFACES
                .iter()
                .map(|s| s.as_str())
                .collect();
            let query = OptionsQuery {
                tier: &tier,
                months,
                country: &country,
                currency: &currency,
                surfaces: &surfaces,
            };
            let result = TokenServer::new(&token_url, &transport)
                .and_then(|server| server.checkout_options(&query))
                .map_err(|e| match buyer_refused(&e) {
                    Some(said) => OptionsFailed::Refused(said),
                    None => OptionsFailed::Failed(e.to_string()),
                });
            Outcome::Options {
                serial,
                asked,
                result,
            }
        }
        Job::Surface {
            serial,
            token_url,
            checkout_id,
            kind,
            choice,
            look,
        } => Outcome::Surface {
            serial,
            result: switch_surface(&token_url, &checkout_id, kind, &choice, &look),
        },
        Job::Abandon {
            token_url,
            checkout_id,
            keyring,
        } => Outcome::Abandoned {
            result: abandon(&token_url, &checkout_id, &keyring),
            checkout_id,
        },
        Job::AwaitPayment {
            serial,
            checkout,
            token_url,
            keyring,
            cancel,
        } => await_payment(serial, &checkout, &token_url, &keyring, &cancel),
        Job::Claims {
            keyring,
            token_url,
            store,
            drives,
            cash_every,
        } => claim_pending(&keyring, token_url.as_deref(), &store, &drives, cash_every, sender),
        Job::FinishCheckout {
            keyring,
            store,
            azlin,
            checkout,
            drive_id,
            grant,
            token_url,
        } => finish_checkout(
            &keyring,
            &store,
            azlin.as_ref(),
            &checkout,
            &drive_id,
            grant.as_ref(),
            &token_url,
        ),
        Job::RedeemPeriods {
            keyring,
            store,
            drives,
        } => redeem_periods(&keyring, &store, &drives),
        Job::ArchiveProve {
            token_url,
            keyring,
            drive_id,
            via,
            checkout,
        } => Outcome::ArchiveProven {
            result: crate::archive::prove(&token_url, &keyring, &drive_id, &via, checkout.as_deref()),
            drive_id,
            checkout,
        },
        Job::RegisterDeviceKey {
            token_url,
            keyring,
            drive_id,
        } => Outcome::DeviceKeyRegistered {
            result: crate::archive::register_device_key_now(&token_url, &keyring, &drive_id),
            drive_id,
        },
        Job::Rescue {
            drive_id,
            source,
            folder,
            home,
            state,
        } => {
            let mut progress = |copied: usize, total: usize| {
                send(
                    sender,
                    Outcome::RescueProgress {
                        drive_id: drive_id.clone(),
                        copied,
                        total,
                    },
                );
            };
            let free = crate::archive::free_bytes(&home);
            let report =
                crate::archive::run_rescue(&*source, &folder, free, state.as_deref(), &mut progress);
            Outcome::RescueDone { drive_id, report }
        }
        Job::SaveSettings { drive, start, mine } => Outcome::SettingsSaved {
            result: crate::save_settings_merged(&drive, &start, &mine),
            sent: mine,
        },
        Job::Thumbnails {
            drive,
            items,
            max_px,
        } => {
            for (key, size) in items {
                // A scanned row's size is not known before its stat: the file's own, here.
                let size = size.or_else(|| {
                    drive
                        .local_path(&key)
                        .and_then(|path| fs::metadata(path).ok())
                        .map(|meta| meta.len())
                });
                let image = if size.is_some_and(|s| s <= THUMBNAIL_MAX_BYTES) {
                    make_thumbnail(&*drive, &key, max_px)
                } else {
                    None
                };
                send(sender, Outcome::Thumbnail { key, image });
            }
            Outcome::ThumbnailsDone
        }
        Job::Find {
            serial,
            request,
            prefix,
            index,
            cancel,
        } => {
            if !debounce(&cancel) {
                return FindBatch::new(serial).finish(FindEnd::default());
            }
            let mut emit = |outcome: Outcome| send(sender, outcome);
            match index.filter(|_| request.contents.is_some()) {
                Some(ask) => run_find_indexed(
                    serial,
                    &request,
                    &prefix,
                    &ask,
                    &extractors(),
                    &cancel,
                    &mut emit,
                ),
                None => run_find(serial, &request, &prefix, &cancel, &mut emit),
            }
        }
        Job::IndexDrive {
            drive_id,
            source,
            dir,
            cancel,
        } => {
            let mut emit = |outcome: Outcome| send(sender, outcome);
            match &source {
                IndexSource::Folder(root) => {
                    run_index_update(&drive_id, root, &dir, &extractors(), &cancel, &mut emit)
                }
                IndexSource::Drive(source) => run_drive_index_update(
                    &drive_id,
                    source,
                    &dir,
                    &extractors(),
                    &cancel,
                    &mut emit,
                ),
            }
        }
        Job::RemoveIndex { drive_id, dir } => {
            let error = match fs::remove_dir_all(&dir) {
                Ok(()) => None,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
                Err(e) => Some(e.to_string()),
            };
            Outcome::IndexRemoved { drive_id, error }
        }
        Job::FindRemote {
            find: search,
            drive,
            cancel,
        } => {
            if !debounce(&cancel) {
                return FindBatch::new(search.serial).finish(FindEnd::default());
            }
            let mut emit = |outcome: Outcome| send(sender, outcome);
            run_find_remote(&search, &*drive, &cancel, &mut emit)
        }
        #[cfg(feature = "encryption")]
        Job::Encryption(job) => Outcome::Encryption(crate::encryption::run(job)),
        Job::BucketSpace { azlin_id, azlin } => Outcome::BucketSpace {
            result: azlin.space().map_err(|e| e.to_string()),
            azlin_id,
        },
        Job::RedeemVoucher {
            serial,
            token_url,
            code,
            tier,
            drive,
            keyring,
        } => redeem_voucher(serial, &token_url, &code, &tier, drive, &keyring),
        Job::RemoveDevice {
            token_url,
            drive_id,
            member,
            keyring,
        } => {
            let transport = AzulTransport::azlin(USER_AGENT);
            let result = match TokenServer::new(&token_url, &transport) {
                Ok(server) => keyring
                    .remove_member(&server, &drive_id, &member)
                    .map_err(|e| cloud_error_text(&e)),
                Err(e) => Err(token_error_text(&e)),
            };
            Outcome::DeviceRemoved {
                drive_id,
                member,
                result,
            }
        }
        Job::Sync(job) => {
            let mut emit = |outcome: Outcome| send(sender, outcome);
            crate::sync_jobs::run(job, &mut emit)
        }
        Job::ReadKeys(round) => Outcome::KeysRead(crate::unlock::read_round(round)),
        Job::RestoreDrive {
            drive_id,
            as_of,
            token_url,
            keyring,
            encrypted,
        } => Outcome::DriveRestored {
            result: crate::restore::run(&drive_id, as_of, &token_url, &keyring, encrypted),
            drive_id,
            as_of,
        },
        Job::PickUp {
            serial,
            checkout,
            keyring,
        } => Outcome::PickedUp {
            serial,
            result: pick_up(&keyring, &checkout),
            checkout,
        },
        Job::Join {
            serial,
            code,
            token_url,
            keyring,
        } => Outcome::Joined {
            serial,
            result: join(&keyring, &token_url, &code),
            code,
        },
    }
}

/// A join code's token spent once for this device's session of the drive (kept in the keyring
/// under the drive's lock), at the configured token server - or the code's when there is none.
fn join(
    keyring: &SharedKeyring,
    token_url: &str,
    code: &azcloud_kit::JoinCode,
) -> Result<azcloud_kit::Joined, String> {
    let url = if token_url.trim().is_empty() {
        code.token_url.as_str()
    } else {
        token_url
    };
    let transport = AzulTransport::azlin(crate::USER_AGENT);
    let server = TokenServer::new(url, &transport).map_err(|e| e.to_string())?;
    keyring.join(&server, code).map_err(|e| e.to_string())
}

/// A voucher: on `drive` the days it adds (under its keyring lock, with its newest token);
/// without one a new drive of `tier` for the Add drive dialog `serial`, its session into the
/// keyring under the drive's lock before anything uses it (as a test drive's).
fn redeem_voucher(
    serial: u64,
    token_url: &str,
    code: &str,
    tier: &str,
    drive: Option<String>,
    keyring: &SharedKeyring,
) -> Outcome {
    let transport = AzulTransport::azlin(USER_AGENT);
    let server = match TokenServer::new(token_url, &transport) {
        Ok(server) => server,
        Err(e) => {
            return match drive {
                Some(drive_id) => Outcome::VoucherRedeemed {
                    drive_id,
                    result: Err(token_error_text(&e)),
                    grant: None,
                },
                None => Outcome::Bought {
                    serial,
                    result: Err(token_error_text(&e)),
                },
            }
        }
    };
    if let Some(drive_id) = drive {
        let redeemed = azcloud_kit::period::redeem_voucher_on_drive(&server, keyring, &drive_id, code)
            .map_err(|e| cloud_error_text(&e))
            .and_then(|answer| answer.map_err(|e| token_error_text(&e)));
        let (result, grant) = match redeemed {
            Ok(VoucherRedeemed::Extended {
                days_added,
                period_until,
                grant,
            }) => (Ok((days_added, period_until)), Some(grant)),
            Ok(VoucherRedeemed::NewDrive(_)) => (Err(Text::key("azdrive-voucher-made-new-drive")), None),
            Err(why) => (Err(why), None),
        };
        return Outcome::VoucherRedeemed {
            drive_id,
            result,
            grant,
        };
    }
    let result = azcloud_kit::period::redeem_voucher_for_new_drive(&server, keyring, code, tier)
        .map_err(|e| token_error_text(&e))
        .and_then(|answer| keep_voucher_drive(keyring, answer));
    Outcome::Bought { serial, result }
}

/// A voucher's new drive into the keyring. The voucher's outbox call is closed once the drive IS
/// in the keyring; a keyring that refuses leaves it open - the drive is handed on unsaved (the
/// dialog retries from its session) and a repeat of the voucher gets the same drive again, never
/// "not valid" for a spent voucher (AZ-P15).
fn keep_voucher_drive(
    keyring: &SharedKeyring,
    answer: azcloud_kit::period::VoucherAnswer,
) -> Result<BoughtDrive, Text> {
    let new_drive = match answer.redeemed() {
        VoucherRedeemed::NewDrive(bundle) => Some((**bundle).clone()),
        VoucherRedeemed::Extended { .. } => None,
    };
    // Days and no drive: nothing was made to store.
    let Some(bundle) = new_drive else {
        let _ = answer.stored();
        return Err(Text::key("azdrive-add-days-not-drive"));
    };
    println!("AZDRIVE_VOUCHER new {}", bundle.drive_id());
    Ok(match keyring.keep_new_drive(&bundle) {
        Ok((session, _)) => {
            let _ = answer.stored();
            BoughtDrive {
                bundle,
                session,
                unsaved: None,
            }
        }
        Err(e) => {
            let _ = answer.unstored();
            BoughtDrive {
                session: bundle.session().to_keyring_secret(),
                bundle,
                unsaved: Some(e.to_string()),
            }
        }
    })
}

/// Runs on a worker thread: the blocking storage call, then its answer to
/// the UI thread (a transfer sends its progress on the way).
pub(crate) extern "C" fn job_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    _receiver: ThreadReceiver,
) {
    let Some((job, inflight)) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut init| Some((init.job.take()?, init.inflight.take())))
    else {
        return;
    };
    let outcome = run_job(job, &mut sender);
    // The claim goes with the answer and ends when the window has handled it: a close that comes
    // before then still waits (`closing::after_job` lets it go afterwards).
    send_message(&mut sender, answered(outcome, inflight));
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

    /// X-AR5b-7: a drive that is not open has tokens that say why they have no vault of the
    /// drive; the grant of a paid checkout then waits in the keyring (AZ-P5). It does not
    /// silently become "this computer only".
    #[test]
    fn x_ar5b_7_a_drive_that_is_not_open_says_why_its_tokens_have_no_vault() {
        let dir = TempDir::new("azdrive-tokens");
        let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
        let tokens = drive_tokens(&store, "d_1", None).expect("this computer's vault");
        let kept = tokens
            .join(&azcloud_kit::VaultState::with_spent("aa"))
            .expect("kept on this computer");
        match kept.drive() {
            azcloud_kit::DriveKeep::NotAvailable(azcloud_kit::NoVault::NotOpened(why)) => {
                assert!(why.contains("not open"), "{why}")
            }
            other => panic!("not a reason: {other:?}"),
        }
        assert!(!kept.is_redundant(), "one file is not two places");
    }

    /// X-DELTA-AZDRIVE-MONEY-6: a Renew order whose drive's vaults cannot all be read is not placed
    /// at today's price: the price lock it would have named may be in the vault that was not read.
    /// The order says so, nothing is sent and no order is begun on the list. Before, `state().ok()?`
    /// made "could not look" the same as "no lock" and the order went out tokens-only, at a price
    /// the buyer's quote - the same list price - agreed with.
    #[test]
    fn x_delta_azdrive_money_6_a_vault_that_cannot_be_read_starts_no_tokens_only_order() {
        let dir = TempDir::new("azdrive-start");
        let store = PeriodTokenStore::new(dir.path().join("period-tokens"));
        // The drive's vault on this computer is cut: whatever lock it held cannot be read.
        fs::create_dir_all(store.dir()).expect("a folder");
        fs::write(store.path_of("d_1").expect("a path"), b"{ cut").expect("a file");
        let keyring = SharedKeyring::with_lock_root(
            Arc::new(azul_storage::keyring::MemoryKeyring::new()),
            azul_storage::keyring::KeyringNamespace::azlin_shared(),
            &dir.path().join("locks"),
        );
        let renew = RenewJob {
            order: crate::renewals::RenewOrder {
                drive_id: String::from("d_1"),
                tier: String::from("100GB"),
                change_tier: false,
                upgrade: false,
            },
            store,
            azlin: None,
        };
        let failed = match start_checkout(
            "http://127.0.0.1:18081",
            "100GB",
            1,
            "sepa",
            "Photos",
            &keyring,
            None,
            Some(&renew),
            None,
        ) {
            Err(failed) => failed,
            Ok(_) => panic!("ordered at today's price without knowing the drive's lock"),
        };
        assert_eq!(failed.why.keys(), vec!["azdrive-order-lock-unread"]);
        assert!(
            pending::list(&keyring).unwrap().is_empty(),
            "no order was begun: nothing was sent"
        );
    }

    /// F5 asks the drive again: a refresh's first page is a `refreshed` listing (an encrypted
    /// drive's index pulls first, so a node that refuses shows); its next pages, and a listing
    /// that opens a folder, are not.
    #[test]
    fn the_first_page_of_a_refresh_asks_the_drive_again() {
        let first = scan_request("docs/", None, true);
        assert!(first.refresh);
        assert_eq!((first.prefix.as_str(), first.max_keys), ("docs/", SCAN_PAGE));
        let next = scan_request("docs/", Some(String::from("t2")), true);
        assert!(!next.refresh);
        assert_eq!(next.continuation.as_deref(), Some("t2"));
        assert!(!scan_request("docs/", None, false).refresh);
    }

    /// A folder with `files` empty files and `folders` subfolders.
    fn folder_with(files: usize, folders: usize) -> TempDir {
        let dir = TempDir::new("azdrive-scan");
        for i in 0..files {
            fs::write(dir.path().join(format!("file-{i:05}.txt")), b"").expect("a file");
        }
        for i in 0..folders {
            fs::create_dir(dir.path().join(format!("sub-{i}"))).expect("a folder");
        }
        dir
    }

    /// Every batch a scan hands over, then its answer.
    fn scan(dir: &Path, prefix: &str, cancel: &AtomicBool) -> (Vec<Outcome>, Outcome) {
        let mut batches = Vec::new();
        let last = scan_dir(dir, prefix, 7, cancel, &mut |o| batches.push(o));
        (batches, last)
    }

    fn rows(outcome: &Outcome) -> &[Entry] {
        match outcome {
            Outcome::Scanned { batch, .. } => batch,
            _ => panic!("not a scan's batch"),
        }
    }

    /// The scan of a big folder hands over its first screen at once (FIRST_BATCH rows) and every
    /// name in the end, in batches; every row knows its kind and nothing else (no stat).
    #[test]
    fn a_scan_hands_over_its_first_screen_at_once_and_every_name_in_batches() {
        let dir = folder_with(listing::FIRST_BATCH + 300, 3);
        let (batches, last) = scan(dir.path(), "docs/", &AtomicBool::new(false));
        assert!(!batches.is_empty(), "a first batch before the end");
        assert_eq!(rows(&batches[0]).len(), listing::FIRST_BATCH);
        assert!(matches!(
            batches[0],
            Outcome::Scanned {
                serial: 7,
                done: false,
                ..
            }
        ));
        assert!(matches!(
            last,
            Outcome::Scanned {
                done: true,
                error: None,
                ..
            }
        ));
        let all: Vec<&Entry> = batches
            .iter()
            .chain([&last])
            .flat_map(|o| rows(o))
            .collect();
        assert_eq!(all.len(), listing::FIRST_BATCH + 300 + 3);
        assert_eq!(all.iter().filter(|e| e.is_folder).count(), 3);
        if cfg!(target_os = "macos") {
            // The folder read carries the sizes and dates with the names: no row waits for a
            // stat.
            assert!(all.iter().all(|e| e.known && e.modified.is_some()));
            assert!(all.iter().filter(|e| !e.is_folder).all(|e| e.size == Some(0)));
        } else {
            assert!(all.iter().all(|e| !e.known && e.size.is_none()));
        }
        assert!(all.iter().any(|e| e.key == "docs/sub-0/"));
        assert!(all.iter().any(|e| e.key == "docs/file-00000.txt"));
    }

    /// Opening a folder of 10,000 files: the first screen of rows is handed over long before
    /// the folder is read, and the whole read costs a few batched calls, not a stat per file
    /// (on macOS the sizes and dates come with the names; elsewhere the rows in view are
    /// stat'ed later, by `Job::Stat`).
    #[test]
    fn a_folder_of_10000_files_shows_its_first_screen_at_once_without_a_stat_per_file() {
        let dir = folder_with(10_000, 0);
        let before = azul_storage::dir_read::calls_on_this_thread();
        let started = Instant::now();
        let mut first_after = None;
        let mut batches = 0;
        let last = scan_dir(dir.path(), "", 7, &AtomicBool::new(false), &mut |_| {
            batches += 1;
            first_after.get_or_insert_with(|| started.elapsed());
        });
        let whole = started.elapsed();
        let after = azul_storage::dir_read::calls_on_this_thread();
        let first = first_after.expect("a first batch before the end");
        println!(
            "AZDRIVE_SCAN_10000 first {first:?} whole {whole:?} batches {batches} \
             batch_reads {} stats {}",
            after.batch_reads - before.batch_reads,
            after.stats - before.stats
        );
        assert!(matches!(last, Outcome::Scanned { done: true, error: None, .. }));
        assert_eq!(after.stats - before.stats, 0, "no stat for any file");
        if cfg!(target_os = "macos") {
            assert!(
                after.batch_reads - before.batch_reads <= 10_000 / 100 + 2,
                "a few hundred entries per call"
            );
        }
        assert!(first < whole, "the first screen did not wait for the folder");
    }

    /// A drive whose second page takes as long as the test wants.
    struct Gated {
        drive: LocalDrive,
        /// Pages answered so far.
        pages: AtomicUsize,
        /// The second page waits for this.
        open: std::sync::Mutex<std::sync::mpsc::Receiver<()>>,
    }

    impl Drive for Gated {
        fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
            if self.pages.fetch_add(1, Ordering::SeqCst) == 1 {
                let _ = self.open.lock().map(|open| open.recv());
            }
            self.drive.list(request)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.drive.get(key)
        }
        fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
            self.drive.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.drive.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.drive.delete(key)
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            self.drive.head(key)
        }
    }

    /// A slow listing (a bucket whose next page takes its time) runs on its own thread and hands
    /// over each page as it comes: the window has the first page's rows, and keeps drawing,
    /// while the second page is still on its way.
    #[test]
    fn a_slow_listing_hands_over_its_first_page_while_the_next_is_still_read() {
        let dir = folder_with(SCAN_PAGE as usize + 10, 0);
        let (release, open) = std::sync::mpsc::channel();
        let drive = Arc::new(Gated {
            drive: LocalDrive::without_manifest(dir.path().to_path_buf()),
            pages: AtomicUsize::new(0),
            open: std::sync::Mutex::new(open),
        });
        let (sent, batches) = std::sync::mpsc::channel::<usize>();
        let scanning = {
            let drive = drive.clone();
            std::thread::spawn(move || {
                let mut emit = |outcome: Outcome| {
                    let _ = sent.send(rows(&outcome).len());
                };
                let last = scan_bucket(&*drive, "", false, 7, &AtomicBool::new(false), &mut emit);
                rows(&last).len()
            })
        };
        let first = batches
            .recv_timeout(Duration::from_secs(20))
            .expect("the first page's rows while the next page is read");
        assert_eq!(first, SCAN_PAGE as usize);
        assert!(!scanning.is_finished(), "the second page is still on its way");
        release.send(()).expect("the listing waits");
        assert_eq!(scanning.join().expect("the scan ends"), 10);
    }

    /// A cancelled scan (the window went elsewhere) stops and hands over nothing more.
    #[test]
    fn a_cancelled_scan_stops_and_hands_over_nothing() {
        let dir = folder_with(50, 0);
        let (batches, last) = scan(dir.path(), "", &AtomicBool::new(true));
        assert!(batches.is_empty());
        assert!(rows(&last).is_empty());
        assert!(matches!(
            last,
            Outcome::Scanned {
                done: true,
                error: None,
                ..
            }
        ));
    }

    /// A folder that cannot be read ends the scan with the reason.
    #[test]
    fn a_scan_of_a_missing_folder_says_why() {
        let dir = TempDir::new("azdrive-scan-missing");
        let (batches, last) = scan(&dir.path().join("gone"), "", &AtomicBool::new(false));
        assert!(batches.is_empty());
        assert!(matches!(
            last,
            Outcome::Scanned {
                done: true,
                error: Some(_),
                ..
            }
        ));
    }

    /// The drive's bookkeeping at its root and the storage crate's temporary files are not rows.
    #[test]
    fn a_scan_leaves_out_the_drives_bookkeeping() {
        let dir = folder_with(1, 0);
        fs::create_dir(dir.path().join(".azlin")).expect("the bookkeeping");
        fs::write(dir.path().join(".a.txt.azul-storage-1-2.tmp"), b"").expect("a temporary file");
        fs::write(dir.path().join(".hidden"), b"").expect("a hidden file");
        let (_, last) = scan(dir.path(), "", &AtomicBool::new(false));
        let mut names: Vec<&str> = rows(&last).iter().map(|e| e.name.as_str()).collect();
        names.sort_unstable();
        assert_eq!(names, vec![".hidden", "file-00000.txt"]);
    }

    /// A stat reads the size and date of the keys asked for; a folder has no size; a key gone
    /// since the scan is left out.
    #[test]
    fn a_stat_reads_the_size_and_date_of_the_rows_asked_for() {
        let dir = TempDir::new("azdrive-stat");
        fs::create_dir_all(dir.path().join("a/b")).expect("folders");
        fs::write(dir.path().join("a/note.txt"), b"hello").expect("a file");
        let note = stat_of(dir.path(), "a/note.txt").expect("the file");
        assert_eq!(note.size, Some(5));
        assert!(note.modified.is_some());
        let folder = stat_of(dir.path(), "a/b/").expect("the folder");
        assert_eq!(folder.size, None);
        assert!(stat_of(dir.path(), "a/gone.txt").is_none());
        assert_eq!(path_in(Path::new("/r"), "a/b/"), PathBuf::from("/r/a/b"));
        assert_eq!(path_in(Path::new("/r"), ""), PathBuf::from("/r"));
    }

    /// Every row a stat job is asked for is answered - one gone since the scan (or one the file
    /// system will not stat, a dangling link) with neither size nor date. A sort by Size or Date
    /// modified waits for an answer for every row: a row that was never answered held it up for
    /// good, and the rows kept the order of the sizes known so far.
    #[test]
    fn every_row_asked_for_is_answered_even_when_its_stat_fails() {
        let dir = TempDir::new("azdrive-stat-gone");
        fs::write(dir.path().join("note.txt"), b"hello").expect("a file");
        let keys = vec![String::from("note.txt"), String::from("gone.txt")];
        let stats = stats_of(dir.path(), &keys);
        assert_eq!(stats.len(), 2, "one answer per row asked for: {stats:?}");
        assert_eq!(stats[0].key, "note.txt");
        assert_eq!(stats[0].size, Some(5));
        assert_eq!(
            stats[1],
            Stat {
                key: String::from("gone.txt"),
                size: None,
                modified: None,
            }
        );
        // Answered, the row is known: the sort that waits for every row can run.
        let mut rows = vec![
            listing::scanned_entry("", "note.txt", false),
            listing::scanned_entry("", "gone.txt", false),
        ];
        assert_eq!(listing::apply_stats(&mut rows, &stats), 2);
        assert!(listing::all_known(&rows));
        assert_eq!(rows[1].size, None);
    }

    /// A folder's items are counted with one read_dir: hidden ones only while they show, never
    /// the bookkeeping at a drive's root.
    #[test]
    fn a_folder_is_counted_without_a_stat_per_item() {
        let dir = folder_with(4, 2);
        fs::write(dir.path().join(".hidden"), b"").expect("a hidden file");
        fs::create_dir(dir.path().join(".azlin")).expect("the bookkeeping");
        assert_eq!(count_items(dir.path(), "", false), Some(6));
        assert_eq!(count_items(dir.path(), "", true), Some(7));
        assert_eq!(count_items(dir.path(), "sub-0/", false), Some(0));
        assert_eq!(count_items(dir.path(), "gone/", false), None);
    }

    /// Every result the search box's search handed over, its batches' and its answer's.
    fn searched(outcomes: &[Outcome]) -> Vec<&crate::find::Found> {
        outcomes
            .iter()
            .flat_map(|o| match o {
                Outcome::Searched { batch, .. } => batch.iter().collect::<Vec<_>>(),
                _ => Vec::new(),
            })
            .collect()
    }

    /// The search box searches the open folder and every folder below it: the names first,
    /// then the files whose contents match ("File contents" on), each a row with the drive's
    /// key, a content match with its line; the answer says it ended.
    #[test]
    fn a_find_streams_the_names_below_the_folder_then_the_files_whose_contents_match() {
        let dir = TempDir::new("azdrive-find");
        fs::create_dir_all(dir.path().join("Docs/deep/er")).expect("folders");
        fs::write(dir.path().join("Docs/deep/er/needle-report.txt"), b"nothing\n").expect("a file");
        fs::write(dir.path().join("Docs/plan.md"), b"one\nthe needle line\n").expect("a file");
        fs::write(dir.path().join("Docs/other.txt"), b"nothing\n").expect("a file");
        let request = crate::find::local_request(
            dir.path().join("Docs"),
            "needle",
            false,
            &crate::find::FindOptions {
                contents: true,
                ..crate::find::FindOptions::default()
            },
        );
        let cancel = AtomicBool::new(false);
        let mut outcomes = Vec::new();
        let last = run_find(7, &request, "Docs/", &cancel, &mut |o| outcomes.push(o));
        outcomes.push(last);
        let found = searched(&outcomes);
        let keys: Vec<&str> = found.iter().map(|f| f.entry.key.as_str()).collect();
        assert_eq!(keys, vec!["Docs/deep/er/needle-report.txt", "Docs/plan.md"], "names first");
        let line = found[1].line.as_ref().expect("the matching line");
        assert_eq!((line.line, &line.text[line.start..line.end]), (2, "needle"));
        assert!(matches!(
            outcomes.last(),
            Some(Outcome::Searched { serial: 7, end: Some(end), .. })
                if end.error.is_none() && !end.limited
        ));
    }

    /// A cloud search as the window asks for it.
    fn remote(
        serial: u64,
        prefix: &str,
        pattern: azul_search::Pattern,
        options: &crate::find::FindOptions,
        cache: Option<PathBuf>,
    ) -> RemoteFind {
        RemoteFind {
            serial,
            prefix: prefix.to_string(),
            pattern,
            options: options.clone(),
            cache,
            drive_index: false,
            contents: None,
        }
    }

    /// A drive that records the listings asked of it (the drive underneath answers).
    struct Recorded {
        drive: LocalDrive,
        asked: std::sync::Mutex<Vec<ListRequest>>,
    }

    impl Drive for Recorded {
        fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
            if let Ok(mut asked) = self.asked.lock() {
                asked.push(request.clone());
            }
            self.drive.list(request)
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            self.drive.get(key)
        }
        fn get_range(&self, key: &str, range: ByteRange) -> Result<Vec<u8>, DriveError> {
            self.drive.get_range(key, range)
        }
        fn put(&self, key: &str, bytes: &[u8]) -> Result<(), DriveError> {
            self.drive.put(key, bytes)
        }
        fn delete(&self, key: &str) -> Result<(), DriveError> {
            self.drive.delete(key)
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            self.drive.head(key)
        }
    }

    /// The sync's answers as a test sets them: the local copies by key, the states by key.
    #[derive(Default)]
    struct Copies {
        copies: std::collections::HashMap<String, PathBuf>,
        states: std::collections::HashMap<String, crate::sync_lookup::SyncState>,
    }

    impl crate::sync_lookup::SyncLookup for Copies {
        fn local_copy(&self, _drive_id: &str, key: &str) -> Option<PathBuf> {
            self.copies.get(key).cloned()
        }
        fn sync_state(&self, _drive_id: &str, key: &str) -> Option<crate::sync_lookup::SyncState> {
            self.states.get(key).copied()
        }
    }

    /// An encrypted drive's names come from its drive index: the folder in one recursive
    /// listing (the index answers it on this computer - no listing side by side), and nothing
    /// is kept on disk even when a cache file is named.
    #[test]
    fn an_encrypted_drives_names_come_from_its_index_in_one_listing_and_none_is_kept() {
        use azul_search::Pattern;

        let dir = TempDir::new("azdrive-find-encrypted");
        let cache = TempDir::new("azdrive-find-encrypted-cache");
        for folder in ["a", "b", "c"] {
            fs::create_dir_all(dir.path().join(folder)).expect("a folder");
            fs::write(dir.path().join(format!("{folder}/report-{folder}.txt")), b"x").expect("a file");
        }
        let drive = Recorded {
            drive: LocalDrive::without_manifest(dir.path().to_path_buf()),
            asked: std::sync::Mutex::new(Vec::new()),
        };
        let cache_file = cache.path().join("drive.tsv");
        let mut search = remote(
            5,
            "",
            Pattern::literal("report"),
            &crate::find::FindOptions::default(),
            Some(cache_file.clone()),
        );
        search.drive_index = true;
        let cancel = AtomicBool::new(false);
        let mut outcomes = Vec::new();
        let last = run_find_remote(&search, &drive, &cancel, &mut |o| outcomes.push(o));
        outcomes.push(last);
        let mut keys: Vec<String> = searched(&outcomes).iter().map(|f| f.entry.key.clone()).collect();
        keys.sort();
        assert_eq!(keys, vec!["a/report-a.txt", "b/report-b.txt", "c/report-c.txt"]);
        let asked = drive.asked.lock().map(|a| a.clone()).unwrap_or_default();
        assert!(!asked.is_empty());
        assert!(
            asked.iter().all(|r| r.prefix.is_empty() && r.delimiter.is_none()),
            "one recursive listing of the folder: {asked:?}"
        );
        assert!(!cache_file.exists(), "an encrypted drive's names are never kept on disk");
    }

    /// A drive whose files are not on this computer (a cloud drive, an encrypted drive) is
    /// indexed from its listing: a file with a local copy is read there (its plain text, never
    /// the bucket); the others only when downloads are allowed, each at most `download_cap`
    /// bytes - read and dropped, nothing kept but the index.
    #[test]
    fn a_drives_index_reads_its_local_copies_and_downloads_within_the_cap_when_allowed() {
        let bucket = TempDir::new("azdrive-index-bucket");
        let copies = TempDir::new("azdrive-index-copies");
        let dir = TempDir::new("azdrive-index-of-drive");
        fs::create_dir_all(bucket.path().join("notes")).expect("a folder");
        fs::write(bucket.path().join("notes/a.txt"), b"alpha in the cloud\n").expect("a file");
        fs::write(bucket.path().join("notes/b.txt"), b"beta\n").expect("a file");
        fs::write(bucket.path().join("big.txt"), b"gamma gamma gamma gamma\n").expect("a file");
        fs::write(copies.path().join("a-copy.txt"), b"alpha copied here\n").expect("a copy");
        let mut sync = Copies::default();
        sync.copies
            .insert(String::from("notes/a.txt"), copies.path().join("a-copy.txt"));
        let drive: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(bucket.path().to_path_buf()));
        let sync: Arc<dyn crate::sync_lookup::SyncLookup> = Arc::new(sync);
        let none = azul_search_index::Extractors::default();
        let cancel = AtomicBool::new(false);
        let source = |download_cap| DriveSource {
            drive: drive.clone(),
            sync: sync.clone(),
            download_cap,
        };
        let held = |outcome: Outcome| match outcome {
            Outcome::Indexed {
                result: Ok((summary, status)),
                ..
            } => (summary, status),
            _ => panic!("the update did not end well"),
        };
        let (_, status) = held(run_drive_index_update(
            "cloud",
            &source(None),
            dir.path(),
            &none,
            &cancel,
            &mut |_| {},
        ));
        assert_eq!(status.files, 1, "only the local copy is read");
        let index = azul_search_index::DriveIndex::open(dir.path()).expect("the index");
        assert_eq!(index.query("copied", "", 10).expect("q"), vec!["notes/a.txt"]);
        assert!(index.query("beta", "", 10).expect("q").is_empty(), "not downloaded");
        drop(index);

        let (summary, _) = held(run_drive_index_update(
            "cloud",
            &source(Some(10)),
            dir.path(),
            &none,
            &cancel,
            &mut |_| {},
        ));
        assert_eq!(summary.indexed, 1, "notes/b.txt, downloaded; big.txt is over the cap");
        let index = azul_search_index::DriveIndex::open(dir.path()).expect("the index");
        assert_eq!(index.query("beta", "", 10).expect("q"), vec!["notes/b.txt"]);
        assert!(index.query("gamma", "", 10).expect("q").is_empty(), "over the cap");
        assert_eq!(index.query("copied", "", 10).expect("q"), vec!["notes/a.txt"]);
    }

    /// A cloud or encrypted drive with an index: its search finds the names as before, then the
    /// files whose text holds the words from the index - a local copy's line read there, a
    /// downloaded file's without one -, each once.
    #[test]
    fn a_drives_contents_come_from_its_index_with_the_lines_of_its_local_copies() {
        use azul_search::Pattern;

        let bucket = TempDir::new("azdrive-contents-bucket");
        let copies = TempDir::new("azdrive-contents-copies");
        let dir = TempDir::new("azdrive-contents-index");
        fs::create_dir_all(bucket.path().join("Docs")).expect("a folder");
        fs::write(bucket.path().join("Docs/a.txt"), b"one\nthe walrus line\n").expect("a file");
        fs::write(bucket.path().join("Docs/b.txt"), b"a walrus too\n").expect("a file");
        fs::write(bucket.path().join("Docs/c.txt"), b"nothing\n").expect("a file");
        fs::write(copies.path().join("a.txt"), b"one\nthe walrus line\n").expect("a copy");
        let mut sync = Copies::default();
        sync.copies.insert(String::from("Docs/a.txt"), copies.path().join("a.txt"));
        let drive: Arc<dyn Drive> = Arc::new(LocalDrive::without_manifest(bucket.path().to_path_buf()));
        let sync: Arc<dyn crate::sync_lookup::SyncLookup> = Arc::new(sync);
        let none = azul_search_index::Extractors::default();
        let cancel = AtomicBool::new(false);
        let source = DriveSource {
            drive: drive.clone(),
            sync: sync.clone(),
            download_cap: Some(1024),
        };
        run_drive_index_update("cloud", &source, dir.path(), &none, &cancel, &mut |_| {});

        let mut search = remote(
            6,
            "Docs/",
            Pattern::literal("walrus"),
            &crate::find::FindOptions {
                contents: true,
                ..crate::find::FindOptions::default()
            },
            None,
        );
        search.contents = Some(crate::find::RemoteContents {
            dir: dir.path().to_path_buf(),
            drive_id: String::from("cloud"),
            sync: sync.clone(),
        });
        let mut outcomes = Vec::new();
        let last = run_find_remote(&search, &*drive, &cancel, &mut |o| outcomes.push(o));
        outcomes.push(last);
        let found = searched(&outcomes);
        let mut keys: Vec<&str> = found.iter().map(|f| f.entry.key.as_str()).collect();
        keys.sort_unstable();
        assert_eq!(keys, vec!["Docs/a.txt", "Docs/b.txt"]);
        let a = found.iter().find(|f| f.entry.key == "Docs/a.txt").expect("a.txt");
        let line = a.line.as_ref().expect("the local copy's line");
        assert_eq!((line.line, line.text.as_str()), (2, "the walrus line"));
        let b = found.iter().find(|f| f.entry.key == "Docs/b.txt").expect("b.txt");
        assert!(b.line.is_none(), "no copy here: found by the index, no line");
        assert_eq!(b.entry.size, Some(13), "its size from the listing");
        assert!(end_of(&outcomes).error.is_none());
    }

    /// The end of a search's last answer.
    fn end_of(outcomes: &[Outcome]) -> crate::find::FindEnd {
        match outcomes.last() {
            Some(Outcome::Searched { end: Some(end), .. }) => end.clone(),
            _ => panic!("the search did not end"),
        }
    }

    /// A cloud drive's folders are listed side by side (the folder's own level first, then each
    /// subfolder's listing on a worker): every match is found, each once.
    #[test]
    fn a_cloud_drive_is_listed_folder_by_folder_in_parallel() {
        use azul_search::Pattern;

        let dir = TempDir::new("azdrive-find-parallel");
        let mut expected = Vec::new();
        for folder in 0..6 {
            for file in 0..4 {
                let key = format!("f{folder}/sub/file-{file}.txt");
                fs::create_dir_all(dir.path().join(format!("f{folder}/sub"))).expect("folders");
                fs::write(dir.path().join(&key), b"x").expect("a file");
                expected.push(key);
            }
        }
        fs::write(dir.path().join("file-top.txt"), b"x").expect("a file");
        expected.push(String::from("file-top.txt"));
        expected.sort();
        let drive = LocalDrive::without_manifest(dir.path().to_path_buf());
        let cancel = AtomicBool::new(false);
        let mut outcomes = Vec::new();
        let last = run_find_remote(
            &remote(3, "", Pattern::literal("file"), &crate::find::FindOptions::default(), None),
            &drive,
            &cancel,
            &mut |o| outcomes.push(o),
        );
        outcomes.push(last);
        let mut keys: Vec<String> = searched(&outcomes)
            .iter()
            .map(|f| f.entry.key.clone())
            .collect();
        keys.sort();
        assert_eq!(keys, expected, "every file once");
        assert!(end_of(&outcomes).error.is_none());
    }

    /// The last full listing of a cloud drive is kept: the next search shows its names at once
    /// (the Cached phase), then the fresh listing's new ones, and ends with the ones it no longer
    /// has (stale: deleted since).
    #[test]
    fn a_second_cloud_search_shows_the_last_listing_first_then_what_changed() {
        use azul_search::Pattern;

        let dir = TempDir::new("azdrive-find-cached");
        let cache = TempDir::new("azdrive-find-cache");
        let cache_file = cache.path().join("drive.tsv");
        fs::create_dir_all(dir.path().join("Docs/old")).expect("folders");
        fs::write(dir.path().join("Docs/old/report-gone.txt"), b"x").expect("a file");
        fs::write(dir.path().join("Docs/report-kept.txt"), b"x").expect("a file");
        let drive = LocalDrive::without_manifest(dir.path().to_path_buf());
        let cancel = AtomicBool::new(false);
        let options = crate::find::FindOptions::default();
        let search = || remote(1, "", Pattern::literal("report"), &options, Some(cache_file.clone()));
        let mut first = Vec::new();
        let last = run_find_remote(&search(), &drive, &cancel, &mut |o| first.push(o));
        first.push(last);
        assert!(end_of(&first).stale.is_empty());
        assert!(cache_file.exists(), "the full listing is kept");

        fs::remove_file(dir.path().join("Docs/old/report-gone.txt")).expect("deleted");
        fs::write(dir.path().join("Docs/report-new.txt"), b"x").expect("a new file");
        let mut second = Vec::new();
        let last = run_find_remote(&search(), &drive, &cancel, &mut |o| second.push(o));
        second.push(last);
        let cached: Vec<String> = second
            .iter()
            .filter_map(|o| match o {
                Outcome::Searched {
                    phase: crate::find::FindPhase::Cached,
                    batch,
                    ..
                } => Some(batch.iter().map(|f| f.entry.key.clone()).collect::<Vec<_>>()),
                _ => None,
            })
            .flatten()
            .collect();
        assert!(cached.iter().any(|k| k == "Docs/old/report-gone.txt"), "shown at once: {cached:?}");
        let keys: Vec<String> = searched(&second).iter().map(|f| f.entry.key.clone()).collect();
        assert!(keys.iter().any(|k| k == "Docs/report-new.txt"), "the new one: {keys:?}");
        assert_eq!(
            keys.iter().filter(|k| *k == "Docs/report-kept.txt").count(),
            1,
            "a cached result is not handed over again"
        );
        assert_eq!(end_of(&second).stale, vec![String::from("Docs/old/report-gone.txt")]);
    }

    /// A drive with an index: the names first as before, then the contents - the index's
    /// files at once (a text's line read again, a mail's from its text), then the files it has
    /// not read as they are now (new since its update) -, each once with its line; nothing
    /// outside the searched folder.
    #[test]
    fn an_indexed_drive_finds_contents_from_its_index_then_reads_what_changed() {
        let drive = TempDir::new("azdrive-indexed");
        let index_dir = TempDir::new("azdrive-index");
        fs::create_dir_all(drive.path().join("Docs/mail")).expect("folders");
        fs::write(drive.path().join("Docs/needle-notes.md"), b"nothing\n").expect("a file");
        fs::write(drive.path().join("Docs/plan.txt"), b"one\nthe needle plan\n").expect("a file");
        fs::write(drive.path().join("Docs/other.txt"), b"nothing\n").expect("a file");
        fs::write(
            drive.path().join("Docs/mail/0001.eml"),
            b"Subject: Needle lunch\r\nFrom: ada@example.org\r\n\r\nPasta at noon?\r\n",
        )
        .expect("a mail");
        fs::write(drive.path().join("Elsewhere.txt"), b"a needle outside\n").expect("a file");
        let none = azul_search_index::Extractors::default();
        let cancel = AtomicBool::new(false);
        let index = azul_search_index::DriveIndex::open(index_dir.path()).expect("the index");
        index
            .update(drive.path(), &crate::find::index_filters(), &none, &cancel, &mut |_| {})
            .expect("indexed");
        drop(index);
        fs::write(drive.path().join("Docs/new.txt"), b"a needle arrives\n").expect("a new file");

        let request = crate::find::local_request(
            drive.path().join("Docs"),
            "needle",
            false,
            &crate::find::FindOptions {
                contents: true,
                ..crate::find::FindOptions::default()
            },
        );
        let ask = crate::find::IndexAsk {
            dir: index_dir.path().to_path_buf(),
            root: drive.path().to_path_buf(),
            under: String::from("Docs/"),
        };
        let mut outcomes = Vec::new();
        let last = run_find_indexed(4, &request, "Docs/", &ask, &none, &cancel, &mut |o| {
            outcomes.push(o)
        });
        outcomes.push(last);
        let found = searched(&outcomes);
        let keys: Vec<&str> = found.iter().map(|f| f.entry.key.as_str()).collect();
        assert_eq!(keys.len(), 4, "{keys:?}");
        assert_eq!(keys[0], "Docs/needle-notes.md", "the names first");
        let mut from_index = keys[1..3].to_vec();
        from_index.sort_unstable();
        assert_eq!(from_index, vec!["Docs/mail/0001.eml", "Docs/plan.txt"], "the index's");
        assert_eq!(keys[3], "Docs/new.txt", "then what it has not read");
        let line = |key: &str| {
            found
                .iter()
                .find(|f| f.entry.key == key)
                .and_then(|f| f.line.clone())
                .unwrap_or_else(|| panic!("{key} has a line"))
        };
        assert_eq!((line("Docs/plan.txt").line, line("Docs/plan.txt").text.as_str()), (2, "the needle plan"));
        let mail = line("Docs/mail/0001.eml");
        assert_eq!(&mail.text[mail.start..mail.end], "Needle");
        assert_eq!(line("Docs/new.txt").line, 1);
        assert!(end_of(&outcomes).error.is_none() && !end_of(&outcomes).limited);
    }

    /// A drive's index update says how far it got, and ends with what it read and what the
    /// index holds now (no hidden items: the search box's defaults).
    #[test]
    fn an_index_update_says_how_far_it_got_and_what_the_index_holds() {
        let drive = TempDir::new("azdrive-index-update");
        let dir = TempDir::new("azdrive-index-folder");
        fs::write(drive.path().join("a.txt"), b"alpha\n").expect("a file");
        fs::write(drive.path().join("b.txt"), b"beta\n").expect("a file");
        fs::write(drive.path().join(".hidden.txt"), b"gamma\n").expect("a hidden file");
        let cancel = AtomicBool::new(false);
        let mut outcomes = Vec::new();
        let last = run_index_update(
            "home",
            drive.path(),
            dir.path(),
            &azul_search_index::Extractors::default(),
            &cancel,
            &mut |o| outcomes.push(o),
        );
        assert!(outcomes
            .iter()
            .any(|o| matches!(o, Outcome::IndexProgress { drive_id, .. } if drive_id == "home")));
        match last {
            Outcome::Indexed {
                drive_id,
                result: Ok((summary, status)),
            } => {
                assert_eq!(drive_id, "home");
                assert_eq!((summary.indexed, status.files), (2, 2), "hidden items are not read");
                assert!(status.updated.is_some());
            }
            _ => panic!("the update did not end well"),
        }
    }

    /// A PDF's text comes through azul's reader; bytes that are no PDF have none.
    #[test]
    fn bytes_that_are_no_pdf_have_no_text() {
        assert_eq!(pdf_text(b"plain words, no PDF"), None);
    }

    /// A search cancelled (a new key, Escape, another folder) hands over nothing.
    #[test]
    fn a_cancelled_find_hands_over_nothing() {
        let dir = folder_with(20, 2);
        let request = crate::find::local_request(
            dir.path().to_path_buf(),
            "file",
            true,
            &crate::find::FindOptions::default(),
        );
        let cancel = AtomicBool::new(true);
        let mut outcomes = Vec::new();
        let last = run_find(3, &request, "", &cancel, &mut |o| outcomes.push(o));
        outcomes.push(last);
        assert!(searched(&outcomes).is_empty());
    }

    /// A cloud drive is searched by name over a recursive listing, page by page (here a folder
    /// on disk seen through the Drive trait alone); a pattern that does not compile ends the
    /// search with why.
    #[test]
    fn a_cloud_drive_is_searched_by_name_over_its_listing() {
        use azul_search::Pattern;

        let dir = TempDir::new("azdrive-find-remote");
        fs::create_dir_all(dir.path().join("Docs/deep")).expect("folders");
        fs::write(dir.path().join("Docs/deep/report.txt"), b"x").expect("a file");
        fs::write(dir.path().join("Docs/notes.txt"), b"x").expect("a file");
        fs::write(dir.path().join("elsewhere-report.txt"), b"x").expect("a file");
        let drive = LocalDrive::without_manifest(dir.path().to_path_buf());
        let cancel = AtomicBool::new(false);
        let mut outcomes = Vec::new();
        let every_folder = crate::find::FindOptions::default();
        let last = run_find_remote(
            &remote(9, "Docs/", Pattern::literal("report"), &every_folder, None),
            &drive,
            &cancel,
            &mut |o| outcomes.push(o),
        );
        outcomes.push(last);
        let keys: Vec<&str> = searched(&outcomes)
            .iter()
            .map(|f| f.entry.key.as_str())
            .collect();
        assert_eq!(keys, vec!["Docs/deep/report.txt"]);
        // The folder alone: its own listing, not the deep report.
        let mut here = Vec::new();
        let this_folder = crate::find::FindOptions {
            subfolders: false,
            ..crate::find::FindOptions::default()
        };
        let last = run_find_remote(
            &remote(11, "", Pattern::literal("report"), &this_folder, None),
            &drive,
            &cancel,
            &mut |o| here.push(o),
        );
        here.push(last);
        let keys: Vec<&str> = searched(&here).iter().map(|f| f.entry.key.as_str()).collect();
        assert_eq!(keys, vec!["elsewhere-report.txt"]);
        let bad = run_find_remote(
            &remote(10, "", Pattern::regex("("), &every_folder, None),
            &drive,
            &cancel,
            &mut |_| {},
        );
        assert!(matches!(
            bad,
            Outcome::Searched {
                end: Some(crate::find::FindEnd { error: Some(_), .. }),
                ..
            }
        ));
    }

    /// A tree node lists its folders (sorted without case) and counts its items with one
    /// read_dir: a folder of many files expands at once.
    #[test]
    fn a_tree_node_lists_its_folders_with_one_read_dir() {
        let dir = folder_with(30, 0);
        fs::create_dir(dir.path().join("beta")).expect("a folder");
        fs::create_dir(dir.path().join("Alpha")).expect("a folder");
        let drive = LocalDrive::without_manifest(dir.path().to_path_buf());
        let node: TreeKey = (String::from("home"), String::new());
        let (folders, items) =
            tree_folders(&drive, Some(dir.path()), &node).expect("the node's folders");
        assert_eq!(folders, vec!["Alpha/", "beta/"]);
        assert_eq!(items, 32);
        let deeper: TreeKey = (String::from("home"), String::from("beta/"));
        let (none, zero) =
            tree_folders(&drive, Some(&dir.path().join("beta")), &deeper).expect("empty");
        assert!(none.is_empty() && zero == 0);
    }

    // ==== A made checkout is shown only when it is what the buyer was told (RF51/RF52) ====

    use azcloud_kit::{CheckoutFor, Quote};
    use azul_storage::{
        keyring::{KeyringNamespace, MemoryKeyring},
        HttpCall, HttpReply, Transport,
    };

    /// A token server that makes the checkout `ck_1` (PLN 4.49, the payment page it names) and
    /// ends any checkout it is asked to abandon; it records what it was asked.
    struct Orders {
        pay_url: &'static str,
        /// What an abandon is answered: the status and the body (an ended checkout by default).
        abandon: (u16, &'static str),
        calls: std::sync::Mutex<Vec<String>>,
    }

    /// An abandon that ends the checkout.
    const ABANDON_ENDS: (u16, &str) = (200, r#"{"checkout_id": "ck_1", "status": "expired"}"#);

    impl Transport for Orders {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            self.calls.lock().unwrap().push(call.url.clone());
            if call.url.ends_with("/abandon") {
                return Ok(HttpReply {
                    status: self.abandon.0,
                    headers: vec![(
                        String::from("content-type"),
                        String::from("application/json"),
                    )],
                    body: self.abandon.1.as_bytes().to_vec(),
                });
            }
            let body = format!(
                r#"{{"checkout_id": "ck_1", "pay_url": "{}", "tier": "100GB", "months": 1,
                    "amount_cents": 449, "currency": "PLN"}}"#,
                self.pay_url
            );
            Ok(HttpReply {
                status: 200,
                headers: vec![(
                    String::from("content-type"),
                    String::from("application/json"),
                )],
                body: body.into_bytes(),
            })
        }
    }

    fn eur(amount: u64) -> azcloud_kit::azlin_contract::Cents {
        crate::pay_words_tests::cents(amount, "EUR")
    }

    /// The order placed and its checkout made, as `start_checkout` has them when it calls
    /// `finish_start`: the list holds the order's record.
    fn placed(
        dir: &TempDir,
        pay_url: &'static str,
    ) -> (Orders, SharedKeyring, Checkout, PendingCheckout, String) {
        placed_with(dir, pay_url, ABANDON_ENDS)
    }

    /// [`placed`] at a token server that answers an abandon with `abandon`.
    fn placed_with(
        dir: &TempDir,
        pay_url: &'static str,
        abandon: (u16, &'static str),
    ) -> (Orders, SharedKeyring, Checkout, PendingCheckout, String) {
        let keyring = SharedKeyring::with_lock_root(
            Arc::new(MemoryKeyring::new()),
            KeyringNamespace::azlin_shared(),
            &dir.path().join("locks"),
        );
        let orders = Orders {
            pay_url,
            abandon,
            calls: std::sync::Mutex::new(Vec::new()),
        };
        let (checkout, kept, posting_id) = {
            let server = TokenServer::new("http://127.0.0.1:18081", &orders).expect("a server");
            let claim = ClaimKey::generate().expect("a claim key");
            let draft = PendingCheckout::posting(&claim, "100GB", server.base(), "Photos");
            let placed = pending::place_order(&keyring, &draft, |claim| {
                server.checkout("100GB", 1, "sepa", claim, CheckoutFor::NewDrive)
            })
            .expect("placed");
            let kept = PendingCheckout::new(
                &placed.answer.checkout_id,
                &placed.claim,
                "100GB",
                server.base(),
                "Photos",
            );
            (placed.answer, kept, placed.posting_id)
        };
        orders.calls.lock().unwrap().clear();
        (orders, keyring, checkout, kept, posting_id)
    }

    /// AZ-P2: a buyer shown EUR 0.99 is not sent to a checkout that charges PLN 4.49. The
    /// checkout is ended at the token server and leaves the list - nothing of it is shown - and
    /// the page is told its prices are stale.
    #[test]
    fn az_p2_a_checkout_that_charges_other_than_the_quote_is_ended_before_anything_is_shown() {
        let dir = TempDir::new("azdrive-start");
        let (orders, keyring, checkout, kept, posting_id) =
            placed(&dir, "http://127.0.0.1:18081/v1/pay/ck_1");
        let server = TokenServer::new("http://127.0.0.1:18081", &orders).expect("a server");
        let quote = Quote {
            price: eur(99),
            tier: String::from("100GB"),
            months: 1,
            at_most: false,
        };
        let failed = match finish_start(
            &server,
            &keyring,
            &posting_id,
            "100GB",
            1,
            None,
            Some(&quote),
            checkout,
            None,
            kept,
        ) {
            Err(failed) => failed,
            Ok(_) => panic!("shown at another price"),
        };
        assert_eq!(failed.why.keys(), vec!["azdrive-order-price-changed"]);
        assert!(failed.prices_moved, "the prices on the page are stale");
        assert!(
            orders
                .calls
                .lock()
                .unwrap()
                .iter()
                .any(|u| u.ends_with("/v1/checkout/ck_1/abandon")),
            "ended at the token server"
        );
        assert!(
            pending::list(&keyring).unwrap().is_empty(),
            "it left the list: nobody may pay it"
        );
    }

    /// X-AR5b-8 (the decisive branch): a checkout that is not shown (here: the price changed) and
    /// whose abandon cannot end it - the token server says it was paid meanwhile, or leaves it
    /// pending - stays on the keyring's list, in flight, and the buyer is told so in words that say
    /// it may still be paid. Before, the two tests of this branch used an abandon that ended it.
    #[test]
    fn x_ar5b_8_a_checkout_that_is_not_shown_and_cannot_be_ended_stays_in_flight_and_says_so() {
        for (abandon, key) in [
            (
                (
                    409_u16,
                    r#"{"error": "checkout_paid", "message": "the drive comes"}"#,
                ),
                "azdrive-order-not-shown-paid",
            ),
            (
                (200, r#"{"checkout_id": "ck_1", "status": "pending"}"#),
                "azdrive-order-not-shown-open",
            ),
        ] {
            let dir = TempDir::new("azdrive-start");
            let (orders, keyring, checkout, kept, posting_id) =
                placed_with(&dir, "http://127.0.0.1:18081/v1/pay/ck_1", abandon);
            let server = TokenServer::new("http://127.0.0.1:18081", &orders).expect("a server");
            let quote = Quote {
                price: eur(99),
                tier: String::from("100GB"),
                months: 1,
                at_most: false,
            };
            let failed = match finish_start(
                &server,
                &keyring,
                &posting_id,
                "100GB",
                1,
                None,
                Some(&quote),
                checkout,
                None,
                kept,
            ) {
                Err(failed) => failed,
                Ok(_) => panic!("shown at another price"),
            };
            assert_eq!(
                failed.why.keys(),
                vec!["azdrive-order-price-changed", key],
                "the price changed, and the order may still be paid"
            );
            assert!(
                failed
                    .why
                    .phrase(key)
                    .and_then(|phrase| phrase.get("checkout"))
                    .is_some_and(|arg| arg.to_string() == "ck_1"),
                "it names the checkout"
            );
            let listed = pending::list(&keyring).unwrap();
            assert_eq!(listed.len(), 1, "{key}: the claim key stays");
            assert_eq!(listed[0].checkout_id, "ck_1");
            assert!(
                listed[0].in_flight_since.is_some(),
                "{key}: marked in flight"
            );
        }
    }

    /// The token server's answers to a renewal: 409 `lock_used` to an order that names a price
    /// lock, a checkout for the order that does not; it records the bodies it was sent.
    struct LockUsed {
        bodies: std::sync::Mutex<Vec<serde_json::Value>>,
    }

    impl Transport for LockUsed {
        fn send(&self, call: &HttpCall) -> Result<HttpReply, String> {
            let body: serde_json::Value =
                serde_json::from_slice(&call.body).map_err(|e| e.to_string())?;
            let (status, answer) = if body.get("renews").is_some() {
                (
                    409,
                    String::from(r#"{"error": "lock_used", "message": "the lock was used"}"#),
                )
            } else {
                (
                    200,
                    String::from(
                        r#"{"checkout_id": "ck_2", "pay_url": "http://127.0.0.1:18081/v1/pay/ck_2",
                            "tier": "100GB", "months": 1, "amount_cents": 99, "currency": "EUR"}"#,
                    ),
                )
            };
            self.bodies.lock().unwrap().push(body);
            Ok(HttpReply {
                status,
                headers: vec![(
                    String::from("content-type"),
                    String::from("application/json"),
                )],
                body: answer.into_bytes(),
            })
        }
    }

    /// A renewal that names its price lock, which the token server no longer takes (`lock_used`),
    /// goes again without it as period tokens only - the order after the refusal is a new one (the
    /// refusal ended the first one's entry), it names no lock, the checkout it makes is the only
    /// one on the list, and the quote still holds it to the price shown. (Not exercised before:
    /// `start_checkout` makes its own transport.)
    #[test]
    fn x_ar5b_20_a_renewal_whose_lock_is_refused_goes_again_without_it_as_the_only_order() {
        let dir = TempDir::new("azdrive-start");
        let keyring = SharedKeyring::with_lock_root(
            Arc::new(MemoryKeyring::new()),
            KeyringNamespace::azlin_shared(),
            &dir.path().join("locks"),
        );
        let token = LockUsed {
            bodies: std::sync::Mutex::new(Vec::new()),
        };
        let server = TokenServer::new("http://127.0.0.1:18081", &token).expect("a server");
        let renew = RenewJob {
            order: crate::renewals::RenewOrder {
                drive_id: String::from("d_1"),
                tier: String::from("100GB"),
                change_tier: false,
                upgrade: false,
            },
            store: PeriodTokenStore::new(dir.path().join("period-tokens")),
            azlin: None,
        };
        let lock = azcloud_kit::RenewalLock {
            checkout_id: String::from("ck_1"),
            tier: String::from("100GB"),
            issue_key: String::from("a-key"),
            ladder_version: Some(7),
            kept_at: 1,
            used_by: None,
        };
        let quote = Quote {
            price: eur(99),
            tier: String::from("100GB"),
            months: 1,
            at_most: true,
        };
        let started = order_and_start(
            &server,
            "100GB",
            1,
            "sepa",
            "Photos",
            &keyring,
            None,
            Some(&renew),
            Some(lock),
            Some(&quote),
        )
        .ok()
        .expect("the order without the lock");
        assert_eq!(started.checkout.checkout_id, "ck_2");
        assert!(started.kept.renews.is_empty(), "no lock was used");
        assert_eq!(started.kept.for_drive, "d_1");
        let bodies = token.bodies.lock().unwrap();
        assert_eq!(bodies.len(), 2, "the order with the lock, then without it");
        assert_eq!(bodies[0]["renews"]["checkout_id"], "ck_1");
        assert!(bodies[1].get("renews").is_none(), "{}", bodies[1]);
        assert_eq!(bodies[1]["grant"], "period_tokens");
        let listed = pending::list(&keyring).unwrap();
        assert_eq!(
            listed
                .iter()
                .map(|c| c.checkout_id.as_str())
                .collect::<Vec<_>>(),
            vec!["ck_2"],
            "the refused order left no record"
        );
    }

    /// X-DELTA-AZDRIVE-MONEY-10: the price shown is the price of the order shown. A checkout that
    /// charges the amount the buyer saw (PLN 4.49) but is for another tier, or for another number
    /// of months, is ended at the token server before anything of it is shown, and the buyer is
    /// told the order was for something else - not that the price changed. Before, only the
    /// amount and the currency were compared and the checkout was shown.
    #[test]
    fn x_delta_azdrive_money_10_a_checkout_for_another_tier_or_period_is_ended_before_it_is_shown()
    {
        for (tier, months) in [("1TB", 1_u32), ("100GB", 12)] {
            let dir = TempDir::new("azdrive-start");
            let (orders, keyring, checkout, kept, posting_id) =
                placed(&dir, "http://127.0.0.1:18081/v1/pay/ck_1");
            let server = TokenServer::new("http://127.0.0.1:18081", &orders).expect("a server");
            // What the buyer was shown: the amount the checkout charges, for another order.
            let quote = Quote {
                price: crate::pay_words_tests::cents(449, "PLN"),
                tier: String::from(tier),
                months,
                at_most: false,
            };
            let failed = match finish_start(
                &server,
                &keyring,
                &posting_id,
                "100GB",
                1,
                None,
                Some(&quote),
                checkout,
                None,
                kept,
            ) {
                Err(failed) => failed,
                Ok(_) => panic!("{tier} / {months}: shown for another order"),
            };
            assert_eq!(
                failed.why.keys(),
                vec!["azdrive-order-changed"],
                "{tier} / {months}"
            );
            assert!(failed.prices_moved, "the page's prices are stale");
            assert!(
                orders
                    .calls
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|u| u.ends_with("/v1/checkout/ck_1/abandon")),
                "{tier} / {months}: ended at the token server"
            );
            assert!(
                pending::list(&keyring).unwrap().is_empty(),
                "{tier} / {months}: it left the list: nobody may pay it"
            );
        }
    }

    /// AZ-P2: the price shown is the price charged: the order goes on, and its checkout is the
    /// list's entry (not the record of the order).
    #[test]
    fn az_p2_a_checkout_that_charges_what_was_shown_goes_on() {
        let dir = TempDir::new("azdrive-start");
        let (orders, keyring, checkout, kept, posting_id) =
            placed(&dir, "http://127.0.0.1:18081/v1/pay/ck_1");
        let server = TokenServer::new("http://127.0.0.1:18081", &orders).expect("a server");
        let quote = Quote {
            price: crate::pay_words_tests::cents(449, "PLN"),
            tier: String::from("100GB"),
            months: 1,
            at_most: false,
        };
        let started = finish_start(
            &server,
            &keyring,
            &posting_id,
            "100GB",
            1,
            None,
            Some(&quote),
            checkout,
            None,
            kept,
        )
        .ok()
        .expect("what was shown");
        assert_eq!(started.checkout.checkout_id, "ck_1");
        let listed = pending::list(&keyring).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(
            listed[0].checkout_id, "ck_1",
            "the checkout, not the record of the order"
        );
        assert!(!listed[0].is_posting());
        assert!(
            orders.calls.lock().unwrap().is_empty(),
            "nothing was abandoned"
        );
    }

    /// AZ-P3: the v1 payment page is the one this token server builds for the checkout. A
    /// `pay_url` of any other host, scheme or path ends the order and is never opened.
    #[test]
    fn az_p3_a_payment_page_the_token_server_does_not_own_is_never_opened() {
        for pay_url in [
            "file:///etc/passwd",
            "smb://host/x.exe",
            "https://evil.example/v1/pay/ck_1",
            "http://127.0.0.1:18081/v1/pay/ck_2",
        ] {
            let dir = TempDir::new("azdrive-start");
            let (orders, keyring, checkout, kept, posting_id) = placed(&dir, pay_url);
            let server = TokenServer::new("http://127.0.0.1:18081", &orders).expect("a server");
            let failed = match finish_start(
                &server,
                &keyring,
                &posting_id,
                "100GB",
                1,
                None,
                None,
                checkout,
                None,
                kept,
            ) {
                Err(failed) => failed,
                Ok(_) => panic!("{pay_url}: opened"),
            };
            assert_eq!(
                failed.why.keys(),
                vec!["azdrive-order-page-not-ours"],
                "{pay_url}"
            );
            assert!(!failed.prices_moved);
            assert!(
                pending::list(&keyring).unwrap().is_empty(),
                "{pay_url}: ended"
            );
        }
    }

    /// AZ-P10: the wait before the next question grows while the token server does not answer
    /// and is never shorter than it asked.
    #[test]
    fn az_p10_the_wait_between_questions_backs_off_and_obeys_retry_after() {
        // All is well: the base.
        assert_eq!(backoff_secs(3, 0, 60, None), 3);
        // No answer, again and again: doubled, to the cap.
        assert_eq!(backoff_secs(3, 1, 60, None), 6);
        assert_eq!(backoff_secs(3, 2, 60, None), 12);
        assert_eq!(backoff_secs(3, 5, 60, None), 60);
        assert_eq!(backoff_secs(3, 40, 60, None), 60, "never overflows");
        // A rate limit that says a minute is not asked again after three seconds.
        assert_eq!(backoff_secs(3, 1, 60, Some(60)), 60);
        // Heard up to ten minutes, no further.
        assert_eq!(backoff_secs(3, 1, 60, Some(300)), 300);
        assert_eq!(backoff_secs(3, 1, 60, Some(86_400)), 600);
        // A claims round: ten seconds, doubled, to five minutes.
        assert_eq!(backoff_secs(10, 0, 300, None), 10);
        assert_eq!(backoff_secs(10, 3, 300, None), 80);
        assert_eq!(backoff_secs(10, 9, 300, None), 300);
        // A cap below the base never shortens the base.
        assert_eq!(backoff_secs(10, 4, 5, None), 10);
    }

    /// A cloud drive of ONE file whose version a test moves on; every `get` is counted.
    struct Cloud {
        bytes: std::sync::Mutex<Vec<u8>>,
        /// `None`: the drive reports neither an etag nor a date.
        etag: std::sync::Mutex<Option<String>>,
        gets: AtomicUsize,
    }

    impl Cloud {
        fn new(bytes: &[u8], etag: Option<&str>) -> Cloud {
            Cloud {
                bytes: std::sync::Mutex::new(bytes.to_vec()),
                etag: std::sync::Mutex::new(etag.map(str::to_string)),
                gets: AtomicUsize::new(0),
            }
        }

        /// The file changed in the cloud.
        fn change(&self, bytes: &[u8], etag: Option<&str>) {
            *self.bytes.lock().unwrap() = bytes.to_vec();
            *self.etag.lock().unwrap() = etag.map(str::to_string);
        }

        fn gets(&self) -> usize {
            self.gets.load(Ordering::SeqCst)
        }
    }

    impl Drive for Cloud {
        fn list(&self, _request: &ListRequest) -> Result<ListPage, DriveError> {
            Ok(ListPage {
                folders: Vec::new(),
                objects: Vec::new(),
                next: None,
            })
        }
        fn get(&self, _key: &str) -> Result<Vec<u8>, DriveError> {
            self.gets.fetch_add(1, Ordering::SeqCst);
            Ok(self.bytes.lock().unwrap().clone())
        }
        fn get_range(&self, key: &str, _range: ByteRange) -> Result<Vec<u8>, DriveError> {
            self.get(key)
        }
        fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
            Ok(())
        }
        fn delete(&self, _key: &str) -> Result<(), DriveError> {
            Ok(())
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            let etag = self.etag.lock().unwrap().clone();
            Ok(ObjectInfo {
                key: key.to_string(),
                size: self.bytes.lock().unwrap().len() as u64,
                modified: etag.as_ref().map(|_| 1_700_000_000),
                etag,
            })
        }
    }

    #[cfg(unix)]
    fn mode_of(path: &Path) -> u32 {
        use std::os::unix::fs::PermissionsExt;
        fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    /// AZ-W4 / AZ-U14: Open and the video preview wrote the file to `<system temp>/AzDrive-open/
    /// <drive>/<name>` - shared, world-readable, never cleaned, a link planted there followed.
    /// The copy is in scratch now: folders only this user can enter (the root of scratch, the
    /// purpose, the file's own), and the file read-only.
    #[test]
    fn az_w4_a_cloud_file_opens_as_a_private_read_only_copy_in_scratch() {
        let base = TempDir::new("azdrive-open-copy");
        let copies = Scratch::under(base.path(), "open");
        let cloud = Cloud::new(b"cloud bytes", Some("e1"));
        let path = opened_copy(&cloud, &copies, "docs/report.docx").expect("a copy");
        assert!(path.starts_with(copies.dir()), "{path:?}");
        assert_eq!(path.file_name().unwrap(), "report.docx");
        assert_eq!(fs::read(&path).unwrap(), b"cloud bytes");
        assert!(
            fs::metadata(&path).unwrap().permissions().readonly(),
            "a save in place fails loudly (AZ-U13)"
        );
        #[cfg(unix)]
        {
            assert_eq!(mode_of(&path), 0o400);
            assert_eq!(
                mode_of(path.parent().unwrap()),
                0o700,
                "the file's own folder"
            );
            assert_eq!(mode_of(copies.dir()), 0o700, "the purpose's folder");
            assert_eq!(mode_of(base.path()), 0o700, "the root of scratch");
        }
    }

    /// AZ-W20: AzDrive's Open gave a file of a drive no "downloaded from elsewhere" mark, so the system
    /// looked at nothing before it ran an `.exe`, a script or a shortcut of a shared drive. The copy
    /// carries the mark where the system has one (macOS, Windows) and nothing on Linux.
    #[test]
    fn az_w20_a_cloud_file_opened_from_azdrive_carries_the_downloaded_mark_where_there_is_one() {
        let base = TempDir::new("azdrive-open-marked");
        let copies = Scratch::under(base.path(), "open");
        let cloud = Cloud::new(b"MZ not really a program", Some("e1"));
        let path = opened_copy(&cloud, &copies, "tools/setup.exe").expect("a copy");
        assert_eq!(
            azul_storage::quarantine::is_marked(&path),
            azul_storage::quarantine::supported()
        );
    }

    /// AZ-U13: opened again, the same version opens the same copy (the old code made `name (1)`
    /// from the drive's bytes and the user's edit was in the other file); a version the drive
    /// changed since is a new copy and the first is not touched.
    #[test]
    fn az_u13_the_same_version_opens_the_same_copy_and_a_new_version_a_new_one() {
        let base = TempDir::new("azdrive-open-version");
        let copies = Scratch::under(base.path(), "open");
        let cloud = Cloud::new(b"cloud bytes", Some("e1"));
        let first = opened_copy(&cloud, &copies, "docs/report.docx").unwrap();
        let again = opened_copy(&cloud, &copies, "docs/report.docx").unwrap();
        assert_eq!(first, again, "the same version, the same copy");
        assert_eq!(cloud.gets(), 1, "fetched once");

        cloud.change(b"edited in the cloud", Some("e2"));
        let newer = opened_copy(&cloud, &copies, "docs/report.docx").unwrap();
        assert_ne!(newer, first);
        assert_eq!(fs::read(&newer).unwrap(), b"edited in the cloud");
        assert_eq!(
            fs::read(&first).unwrap(),
            b"cloud bytes",
            "the first copy stays as it was"
        );
        assert_eq!(cloud.gets(), 2);
    }

    /// AZ-U13: a drive that reports neither an etag nor a date cannot say a file is the same one:
    /// its copy is made again every time.
    #[test]
    fn az_u13_a_drive_that_cannot_tell_versions_apart_never_reuses_a_copy() {
        let base = TempDir::new("azdrive-open-unversioned");
        let copies = Scratch::under(base.path(), "open");
        let cloud = Cloud::new(b"cloud bytes", None);
        let first = opened_copy(&cloud, &copies, "a.txt").unwrap();
        let second = opened_copy(&cloud, &copies, "a.txt").unwrap();
        assert_ne!(first, second);
        assert_eq!(cloud.gets(), 2);
    }

    /// AZ-W4: a copy somebody changed (made writable and shortened) is not this version's bytes:
    /// it is fetched again, in the same place - and the change is NOT deleted (X-DELTA-AZDRIVE-FILES-7:
    /// it is put aside as `a (edited).txt`). And a copy older than the horizon goes before the
    /// next one is made (the sweep).
    #[cfg(unix)]
    #[test]
    fn az_w4_a_changed_copy_is_fetched_again_and_old_copies_are_swept() {
        use std::os::unix::fs::PermissionsExt;
        let base = TempDir::new("azdrive-open-changed");
        let copies = Scratch::under(base.path(), "open");
        let cloud = Cloud::new(b"cloud bytes", Some("e1"));
        let path = opened_copy(&cloud, &copies, "a.txt").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();
        fs::write(&path, b"x").unwrap();
        let again = opened_copy(&cloud, &copies, "a.txt").unwrap();
        assert_eq!(again, path);
        assert_eq!(fs::read(&again).unwrap(), b"cloud bytes");
        assert_eq!(mode_of(&again), 0o400);
        assert_eq!(
            fs::read(path.with_file_name("a (edited).txt")).unwrap(),
            b"x",
            "the change was deleted"
        );

        let old = copies.keep("an old copy").unwrap();
        fs::write(old.join("old.txt"), b"x").unwrap();
        let long_ago = SystemTime::now() - OPEN_COPY_KEPT * 3;
        // A kept folder is as old as the newest thing in it.
        for path in [old.join("old.txt"), old.clone()] {
            fs::File::open(&path)
                .unwrap()
                .set_modified(long_ago)
                .unwrap();
        }
        cloud.change(b"cloud bytes 2", Some("e2"));
        opened_copy(&cloud, &copies, "a.txt").unwrap();
        assert!(
            !old.exists(),
            "a copy older than a day is swept before the next is made"
        );
        assert!(again.exists(), "a young copy stays");
    }

    /// A copy of the cloud file `docs/report.docx` v1, opened.
    fn opened_report(base: &TempDir) -> (Scratch, Cloud, PathBuf) {
        let copies = Scratch::under(base.path(), "open");
        let cloud = Cloud::new(b"cloud bytes", Some("e1"));
        let path = opened_copy(&cloud, &copies, "docs/report.docx").unwrap();
        (copies, cloud, path)
    }

    /// X-DELTA-AZDRIVE-FILES-7 (AZ-U13). The copy is 0400, which stops a write in place; an editor
    /// that saves a temp file and renames it over the copy needs only the folder (the user's). The
    /// next Open of the file found a copy of another length, took it for "changed" and DELETED it,
    /// then fetched the original: an edit lost silently. It is put aside and said so.
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_files_7_an_edit_saved_over_the_copy_is_kept_when_the_file_is_opened_again() {
        let base = TempDir::new("dx7-over");
        let (copies, cloud, path) = opened_report(&base);
        // The editor's save: a new file of its own, renamed over the read-only copy.
        let temp = path.with_file_name("~wrd0001.tmp");
        fs::write(&temp, b"my important edit, a different length").unwrap();
        fs::rename(&temp, &path).unwrap();
        let opened = opened_copy_keeping_edits(&cloud, &copies, "docs/report.docx").unwrap();
        assert_eq!(opened.path, path);
        assert_eq!(
            fs::read(&opened.path).unwrap(),
            b"cloud bytes",
            "the original again"
        );
        let kept = opened.kept_edit.expect("the edit is said to be kept");
        assert_eq!(kept, path.with_file_name("report (edited).docx"));
        assert_eq!(
            fs::read(&kept).unwrap(),
            b"my important edit, a different length",
            "the edit was deleted"
        );
        // The same again: the second edit gets a name of its own, the first stays.
        let temp = path.with_file_name("~wrd0002.tmp");
        fs::write(&temp, b"another edit").unwrap();
        fs::rename(&temp, &path).unwrap();
        let second = opened_copy_keeping_edits(&cloud, &copies, "docs/report.docx").unwrap();
        assert_eq!(
            second.kept_edit.unwrap(),
            path.with_file_name("report (edited 2).docx")
        );
        assert_eq!(
            fs::read(&kept).unwrap(),
            b"my important edit, a different length"
        );
    }

    /// X-DELTA-AZDRIVE-FILES-7: an edit that keeps the length (a replaced file of the same size) is
    /// not "whole" either: the copy's identity says so.
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_files_7_an_edit_of_the_same_length_is_kept_too() {
        let base = TempDir::new("dx7-same-length");
        let (copies, cloud, path) = opened_report(&base);
        let temp = path.with_file_name("~wrd0001.tmp");
        fs::write(&temp, b"CLOUD BYTES").unwrap();
        assert_eq!(
            fs::metadata(&temp).unwrap().len(),
            b"cloud bytes".len() as u64
        );
        fs::rename(&temp, &path).unwrap();
        let opened = opened_copy_keeping_edits(&cloud, &copies, "docs/report.docx").unwrap();
        let kept = opened.kept_edit.expect("the edit is kept");
        assert_eq!(fs::read(kept).unwrap(), b"CLOUD BYTES");
        assert_eq!(fs::read(opened.path).unwrap(), b"cloud bytes");
    }

    /// X-DELTA-AZDRIVE-FILES-7: no false alarm - a copy nobody touched opens again as it is and
    /// nothing is put aside.
    #[test]
    fn x_delta_azdrive_files_7_an_untouched_copy_is_reused_and_nothing_is_put_aside() {
        let base = TempDir::new("dx7-untouched");
        let (copies, cloud, path) = opened_report(&base);
        let opened = opened_copy_keeping_edits(&cloud, &copies, "docs/report.docx").unwrap();
        assert_eq!(opened.path, path);
        assert!(opened.kept_edit.is_none());
        assert_eq!(cloud.gets(), 1, "fetched once");
    }

    /// X-DELTA-AZDRIVE-FILES-7: a folder that holds work of the user's is never swept, however old;
    /// the same age takes an untouched copy's folder (and one no record names) away.
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_files_7_a_folder_that_holds_an_edit_is_never_swept() {
        let base = TempDir::new("dx7-sweep");
        let (copies, cloud, path) = opened_report(&base);
        let folder = path.parent().unwrap().to_path_buf();
        // A second file's copy, untouched; both folders become three days old.
        let other = opened_copy(&cloud, &copies, "docs/other.docx").unwrap();
        let other_folder = other.parent().unwrap().to_path_buf();
        // The user saves beside the copy ("Save As" in the folder the editor proposes).
        fs::write(folder.join("report - my version.docx"), b"my version").unwrap();
        let long_ago = SystemTime::now() - OPEN_COPY_KEPT * 3;
        // A kept folder is as old as the newest thing in it: everything in them is old.
        for dir in [&folder, &other_folder] {
            for entry in fs::read_dir(dir).unwrap() {
                let file = fs::File::open(entry.unwrap().path()).unwrap();
                file.set_modified(long_ago).unwrap();
            }
            fs::File::open(dir).unwrap().set_modified(long_ago).unwrap();
        }
        let swept = copies.sweep(OPEN_COPY_KEPT);
        assert_eq!(swept, 1, "only the untouched copy's folder went");
        assert!(!other_folder.exists());
        assert_eq!(
            fs::read(folder.join("report - my version.docx")).unwrap(),
            b"my version",
            "the user's file was swept with the copy's folder"
        );
        assert!(path.exists());
    }

    /// AZ-U13: the status line says that a cloud file opens as a read-only copy, and a file on
    /// this computer by its own name.
    #[test]
    fn az_u13_the_status_line_says_a_cloud_file_opens_as_a_read_only_copy() {
        let base = TempDir::new("azdrive-opening-phrase");
        fs::write(base.path().join("a.txt"), b"x").unwrap();
        let local = LocalDrive::without_manifest(base.path().to_path_buf());
        let here = crate::actions::opening_phrase(&local, "a.txt", "a.txt");
        assert_eq!(here.key, "azdrive-opening");
        let cloud = Cloud::new(b"x", Some("e"));
        let there = crate::actions::opening_phrase(&cloud, "a.txt", "a.txt");
        assert_eq!(there.key, "azdrive-opening-copy");
        assert_eq!(
            there.get("name").map(ToString::to_string).as_deref(),
            Some("a.txt")
        );
    }

    /// X-AR5a-23: with no drives file the paid months' tokens were kept in
    /// `<system temp>/azul-storage-period-tokens` - a name every user can guess, in a folder every
    /// user can make first. They wait in a private folder of scratch.
    #[test]
    fn x_ar5a_23_without_a_drives_file_the_period_tokens_wait_in_scratch_not_in_the_temp_folder() {
        let dir = crate::period_tokens_dir(None);
        assert_eq!(dir, Scratch::new("period-tokens").dir());
        assert!(!dir.starts_with(std::env::temp_dir().join("azul-storage-period-tokens")));
        assert!(dir
            .parent()
            .is_some_and(|root| root != std::env::temp_dir()));
        // With a drives file the folder is beside it, as before.
        assert_eq!(
            crate::period_tokens_dir(Some(Path::new("/home/ann/.config/azul/drives.json"))),
            Path::new("/home/ann/.config/azul/period-tokens")
        );
    }

    /// X-DELTA-AZDRIVE-FILES-30. A job's claim was dropped when its thread ended, before the window
    /// read its answer: a close in that gap saw no work, and the answer of a bought drive - which
    /// writes its entry in the drives file - was lost. The claim travels with the answer, is held
    /// while it is handled, and ends with it; an answer nobody reads takes it along.
    #[test]
    fn x_delta_azdrive_files_30_the_claim_travels_with_the_answer_until_it_is_handled() {
        use std::sync::atomic::{AtomicUsize, Ordering};

        let counter = Arc::new(AtomicUsize::new(0));
        let mut message = answered(
            Outcome::ThumbnailsDone,
            Some(crate::closing::Inflight::start(&counter)),
        );
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "the thread is over and the answer is not read yet: the close still waits"
        );
        let claim = message
            .downcast_mut::<Done>()
            .and_then(|mut done| done.inflight.take());
        assert!(claim.is_some());
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "held while it is handled"
        );
        drop(claim);
        assert_eq!(counter.load(Ordering::SeqCst), 0, "handled: the close goes");
        // An answer that is never read (the window went) drops its claim with it.
        let unread = answered(
            Outcome::ThumbnailsDone,
            Some(crate::closing::Inflight::start(&counter)),
        );
        assert_eq!(counter.load(Ordering::SeqCst), 1);
        drop(unread);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        // An answer with no claim (a progress report) holds nothing.
        let progress = answered(Outcome::ThumbnailsDone, None);
        assert_eq!(counter.load(Ordering::SeqCst), 0);
        drop(progress);
    }

    /// X-DELTA-AZDRIVE-FILES-29: the path taken for the vault is made and checked, not only named:
    /// another user's folder made first at the name would have held the paid months' tokens. A
    /// place that cannot be had is a path every system call refuses.
    #[test]
    fn x_delta_azdrive_files_29_the_vault_folder_in_scratch_is_made_and_checked() {
        let dir = crate::period_tokens_dir(None);
        assert_eq!(dir, Scratch::new("period-tokens").dir());
        assert!(dir.is_dir(), "made: {dir:?}");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
            assert_eq!(mode(&dir), 0o700);
            assert_eq!(mode(dir.parent().unwrap()), 0o700);
        }
        assert!(crate::scratch_dir("nodes").is_dir());
        assert!(fs::create_dir_all(azul_storage::scratch::refused_dir()).is_err());
    }

    /// A drive whose look is stale: it says nothing is there (`head`), while `put_if` knows the
    /// name was made since. Every `put` is counted.
    struct Racy {
        puts: AtomicUsize,
    }

    impl Drive for Racy {
        fn list(&self, _request: &ListRequest) -> Result<ListPage, DriveError> {
            Ok(ListPage::default())
        }
        fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
            Err(DriveError::NotFound {
                key: key.to_string(),
            })
        }
        fn get_range(&self, key: &str, _range: ByteRange) -> Result<Vec<u8>, DriveError> {
            self.get(key)
        }
        fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
            self.puts.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
        fn delete(&self, _key: &str) -> Result<(), DriveError> {
            Ok(())
        }
        fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
            Err(DriveError::NotFound {
                key: key.to_string(),
            })
        }
        fn put_if(
            &self,
            key: &str,
            _bytes: &[u8],
            _condition: &Precondition,
        ) -> Result<Option<String>, DriveError> {
            Err(DriveError::Conflict {
                key: key.to_string(),
            })
        }
    }

    /// X-AR5a-9: New text document / New file looked (`exists`) and then wrote (`put`): a file
    /// made in between was replaced by an empty one. The one step the drive offers refuses it.
    #[test]
    fn x_ar5a_9_a_new_file_is_made_only_if_the_name_is_free_in_one_step() {
        let racy = Racy {
            puts: AtomicUsize::new(0),
        };
        let refused = create_empty_file(&racy, "notes.txt").unwrap_err();
        assert!(
            matches!(&refused, DriveError::InvalidKey { reason, .. } if reason.contains("already")),
            "{refused:?}"
        );
        assert_eq!(
            racy.puts.load(Ordering::SeqCst),
            0,
            "nothing was written over it"
        );

        // On a folder on disk: the first is made, the second is refused and the file is untouched.
        let dir = TempDir::new("azdrive-create");
        let home = LocalDrive::without_manifest(dir.path().to_path_buf());
        create_empty_file(&home, "a.txt").expect("made");
        home.put("a.txt", b"mine").unwrap();
        let again = create_empty_file(&home, "a.txt").unwrap_err();
        assert!(matches!(again, DriveError::InvalidKey { .. }), "{again:?}");
        assert_eq!(home.get("a.txt").unwrap(), b"mine");
    }

    /// X-DELTA-AZDRIVE-FILES-21 (X-AR5a-9's twin). Share > Zip chose a name from the loaded listing and
    /// `put` the archive over it: `Photos.zip` that another device synced in (or that the still
    /// loading listing did not show yet) was replaced; a local drive and a plain bucket keep no
    /// version of it. The archive is put only if the name is free and takes the next one otherwise.
    #[test]
    fn x_delta_azdrive_files_21_a_zip_never_replaces_a_file_that_has_its_name() {
        let dir = TempDir::new("dx21-zip");
        let home = LocalDrive::without_manifest(dir.path().to_path_buf());
        home.put("Photos/a.txt", b"alpha").unwrap();
        home.put("Photos.zip", b"the user's own Photos.zip")
            .unwrap();
        let items = [SourceItem {
            key: String::from("Photos/"),
            is_folder: true,
            size: None,
        }];
        let (len, key) = make_zip(&home, &items, "Photos.zip").unwrap();
        assert_eq!(key, "Photos (2).zip", "the next free name");
        assert_eq!(
            home.get("Photos.zip").unwrap(),
            b"the user's own Photos.zip",
            "the file that had the name was replaced"
        );
        assert_eq!(home.get(&key).unwrap().len() as u64, len);
        // A free name is used as it is.
        let (_, key) = make_zip(&home, &items, "Fresh.zip").unwrap();
        assert_eq!(key, "Fresh.zip");
        // On a drive that asks (a name made between the look and the write): the same.
        let racy = Racy {
            puts: AtomicUsize::new(0),
        };
        let refused = put_new_zip(&racy, "Photos.zip", b"zip");
        assert!(
            refused.is_err() || racy.puts.load(Ordering::SeqCst) == 0,
            "{refused:?}"
        );
    }

    /// X-DELTA-AZDRIVE-FILES-21: the archive is built in memory, so a folder bigger than
    /// ZIP_IN_MEMORY_MAX is refused before a byte is read (a 20 GB folder exhausted the memory).
    #[test]
    fn x_delta_azdrive_files_21_a_folder_too_big_for_memory_is_refused_unread() {
        struct Huge {
            gets: AtomicUsize,
        }
        impl Drive for Huge {
            fn list(&self, request: &ListRequest) -> Result<ListPage, DriveError> {
                let mut page = ListPage::default();
                if request.prefix == "big/" {
                    page.objects.push(ObjectInfo {
                        key: String::from("big/log.bin"),
                        size: ZIP_IN_MEMORY_MAX + 1,
                        modified: Some(1),
                        etag: None,
                    });
                }
                Ok(page)
            }
            fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
                self.gets.fetch_add(1, Ordering::SeqCst);
                Err(DriveError::NotFound { key: key.into() })
            }
            fn get_range(&self, key: &str, _range: ByteRange) -> Result<Vec<u8>, DriveError> {
                self.get(key)
            }
            fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
                Ok(())
            }
            fn delete(&self, _key: &str) -> Result<(), DriveError> {
                Ok(())
            }
            fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
                Err(DriveError::NotFound { key: key.into() })
            }
        }
        let huge = Huge {
            gets: AtomicUsize::new(0),
        };
        let items = [SourceItem {
            key: String::from("big/"),
            is_folder: true,
            size: None,
        }];
        let refused = make_zip(&huge, &items, "big.zip").unwrap_err();
        assert!(matches!(refused, DriveError::Unsupported(_)), "{refused:?}");
        assert_eq!(huge.gets.load(Ordering::SeqCst), 0, "a byte was read");
    }

    /// X-DELTA-AZDRIVE-FILES-20 (X-AR5a-10's twin). Ctrl+Z of a New folder decided "still empty" by a
    /// listing, which cannot see a link to a folder (or a socket, or a name that is not UTF-8), and
    /// then `delete_folder` (`remove_dir_all` on a disk) took them with the folder.
    #[cfg(unix)]
    #[test]
    fn x_delta_azdrive_files_20_undoing_a_new_folder_keeps_what_the_listing_cannot_see() {
        let dir = TempDir::new("dx20-undo-folder");
        let home = LocalDrive::without_manifest(dir.path().to_path_buf());
        home.create_folder("New folder/").unwrap();
        // A link to a folder: no listing shows it.
        let link = dir.path().join("New folder").join("link");
        std::os::unix::fs::symlink(dir.path(), &link).unwrap();
        assert!(
            storage_ops::list_all(&home, "New folder/")
                .unwrap()
                .iter()
                .all(|o| o.key == "New folder/"),
            "the listing shows nothing in it"
        );
        let refused = undo_create(&home, "New folder/").unwrap_err();
        assert!(
            matches!(&refused, DriveError::InvalidKey { reason, .. } if reason.contains("not empty")),
            "{refused:?}"
        );
        assert!(
            link.symlink_metadata().is_ok(),
            "the link went with the folder"
        );
        // Once it is really empty, the folder goes.
        std::fs::remove_file(&link).unwrap();
        undo_create(&home, "New folder/").unwrap();
        assert!(!dir.path().join("New folder").exists());
    }

    /// An empty file that was written to between the look and the delete is the user's.
    #[test]
    fn x_delta_azdrive_files_20_undoing_a_new_file_deletes_only_the_empty_one_it_looked_at() {
        struct WrittenAfterTheLook {
            looks: AtomicUsize,
            deleted: AtomicUsize,
        }
        impl Drive for WrittenAfterTheLook {
            fn list(&self, _request: &ListRequest) -> Result<ListPage, DriveError> {
                Ok(ListPage::default())
            }
            fn get(&self, key: &str) -> Result<Vec<u8>, DriveError> {
                Err(DriveError::NotFound { key: key.into() })
            }
            fn get_range(&self, key: &str, _range: ByteRange) -> Result<Vec<u8>, DriveError> {
                self.get(key)
            }
            fn put(&self, _key: &str, _bytes: &[u8]) -> Result<(), DriveError> {
                Ok(())
            }
            fn delete(&self, _key: &str) -> Result<(), DriveError> {
                self.deleted.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
            fn head(&self, key: &str) -> Result<ObjectInfo, DriveError> {
                // The first look: the empty file. Every look after it: the user saved text.
                let first = self.looks.fetch_add(1, Ordering::SeqCst) == 0;
                Ok(ObjectInfo {
                    key: key.to_string(),
                    size: if first { 0 } else { 12 },
                    modified: Some(1),
                    etag: Some(String::from(if first { "v1" } else { "v2" })),
                })
            }
        }
        let drive = WrittenAfterTheLook {
            looks: AtomicUsize::new(0),
            deleted: AtomicUsize::new(0),
        };
        let refused = undo_create(&drive, "notes.txt").unwrap_err();
        assert!(
            matches!(refused, DriveError::InvalidKey { .. }),
            "{refused:?}"
        );
        assert_eq!(
            drive.deleted.load(Ordering::SeqCst),
            0,
            "the file the user wrote to was deleted"
        );
    }

    /// A drive that cannot ask (no conditional put) is looked at first, as before.
    #[test]
    fn x_ar5a_9_a_drive_that_cannot_ask_is_checked_first() {
        let dir = TempDir::new("azdrive-create-plain");
        let (_release, open) = std::sync::mpsc::channel();
        let plain = Gated {
            drive: LocalDrive::without_manifest(dir.path().to_path_buf()),
            pages: AtomicUsize::new(0),
            open: std::sync::Mutex::new(open),
        };
        create_empty_file(&plain, "b.txt").expect("made");
        assert!(dir.path().join("b.txt").is_file());
        let again = create_empty_file(&plain, "b.txt").unwrap_err();
        assert!(matches!(again, DriveError::InvalidKey { .. }), "{again:?}");
    }
}
