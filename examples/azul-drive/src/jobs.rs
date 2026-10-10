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
    pending::{self, Claimed, Finished, PendingTokens, Polled},
    look_at_drive, Checkout, CheckoutVia, ClaimKey, CloudError, DriveBundle, OptionsQuery,
    PendingCheckout, PeriodTokenStore, PeriodTokens, SharedKeyring, Tiers, TokenError,
    TokenServer, UserError, VoucherRedeemed,
};
use azul_pay::{Choice, Created, Look, SurfaceKind};
use azul::{
    image::{ImageRef, RawImage},
    prelude::*,
    vec::U8VecRef,
};
use azul_search_index::{
    DriveIndex, ExtractFn, Extractors, IndexStatus, Kind, UpdateProgress, UpdateSummary,
};
use azul_storage::{
    azul_transport::AzulTransport, config::DriveEntry, ops as storage_ops, transfer, ByteRange,
    Drive, DriveError, ListPage, ListRequest, LocalDrive, ObjectInfo,
};

use crate::{
    browse::{self, Entry},
    fileops::{self, Plan, Progress, SourceItem, TransferKind, TransferReport},
    find::{self, FindEnd, FindPhase},
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
    Text(String),
    /// A local file the video widget plays.
    Video(PathBuf),
    /// A WAV file's samples, for azul's AudioSink.
    Audio(preview::WavSamples),
    /// Why there is nothing to show.
    Message(String),
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
/// the fields page's look, the VAT country, the consent.
pub(crate) struct PayVia {
    pub choice: Choice,
    pub surface: SurfaceKind,
    pub look: Look,
    pub country: String,
    pub consent: bool,
}

/// A checkout made: the token server's answer, the keyring's entry of it, and - through a
/// provider - its checked surface (`None`: the v1 checkout's payment page).
pub(crate) struct Started {
    pub checkout: Checkout,
    pub kept: PendingCheckout,
    pub created: Option<Created>,
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
        same_drive: bool,
        kind: TransferKind,
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
        /// Where a cloud video is fetched to.
        temp_dir: PathBuf,
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
    /// A file fetched to `folder` and handed to the OS's app.
    Open {
        drive: Arc<dyn Drive>,
        key: String,
        size: Option<u64>,
        folder: PathBuf,
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
    /// Buy storage's tier list from the token server at `token_url`.
    Tiers { serial: u64, token_url: String },
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
        name: String,
        keyring: SharedKeyring,
        via: Option<Box<PayVia>>,
    },
    /// Buy storage's payment options (`GET /v1/checkout/options`) for `tier`, `months`, the
    /// payer's `country` and `currency`.
    Options {
        serial: u64,
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
    /// The checkout `checkout_id` abandoned (`POST /v1/checkout/{id}/abandon`) and taken off
    /// the keyring's list: nobody pays it any more.
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
    /// a claimed one's period tokens issued into `store`.
    Claims {
        keyring: SharedKeyring,
        token_url: Option<String>,
        store: PeriodTokenStore,
    },
    /// A claimed checkout whose drive `drive_id` is in the drives file: without a `grant` it
    /// leaves the keyring's list; with one it stays there (its issue key with its claim secret)
    /// until its period tokens are issued at `token_url` and kept in `store`.
    FinishCheckout {
        keyring: SharedKeyring,
        store: PeriodTokenStore,
        checkout: PendingCheckout,
        drive_id: String,
        grant: Option<PeriodTokens>,
        token_url: String,
    },
    /// A look at the Azlin drives `drives` (each one's id and token server): their status (a
    /// pending recovery-key lockdown) and, when their periods near their ends, their next month
    /// from a period token kept in `store` - each under its drive's keyring lock, with its
    /// newest drive token ([`azcloud_kit::look_at_drive`]).
    RedeemPeriods {
        keyring: SharedKeyring,
        store: PeriodTokenStore,
        drives: Vec<(String, String)>,
    },
    /// The settings file written (through a LocalDrive on the config folder).
    SaveSettings {
        drive: LocalDrive,
        text: String,
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
    /// A pending recovery-key lockdown of `drive_id` called off at `token_url` - a grant: under
    /// the drive's keyring lock with its newest drive token.
    CancelLockdown {
        keyring: SharedKeyring,
        drive_id: String,
        token_url: String,
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
    /// The folder sync: a pass, a file opened through it, pins, "Free up space", an answer.
    Sync(crate::sync_jobs::SyncJob),
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
        error: Option<String>,
    },
    /// The sizes and dates a [`Job::Stat`] found.
    Stats { serial: u64, stats: Vec<Stat> },
    /// The item counts a [`Job::Count`] found, by folder key.
    Counted {
        drive_id: String,
        counts: Vec<(String, usize)>,
    },
    /// A tree node's folders (sorted, without case) and how many items it holds in all.
    Folders {
        node: TreeKey,
        result: Result<(Vec<String>, usize), DriveError>,
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
        result: Result<Vec<(String, String)>, DriveError>,
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
    /// Buy storage's tier list (or why there is none).
    Tiers {
        serial: u64,
        result: Result<Tiers, String>,
    },
    /// A development server's test drive (or why there is none).
    Bought {
        serial: u64,
        result: Result<BoughtDrive, String>,
    },
    /// A checkout, on the keyring's list of unfinished checkouts (or why there is none).
    CheckoutStarted {
        serial: u64,
        result: Result<Started, String>,
    },
    /// Buy storage's payment options: the offer's text (`None`: the token server has none).
    Options {
        serial: u64,
        result: Result<Option<String>, String>,
    },
    /// The checkout's next surface, checked (or why there is none).
    Surface {
        serial: u64,
        result: Result<azul_pay::Surface, String>,
    },
    /// A checkout abandoned (or why the token server did not hear it).
    Abandoned {
        checkout_id: String,
        result: Result<(), String>,
    },
    /// The wait for a payment ended without a drive: why (empty: "Stop waiting" said it).
    PaymentEnded { serial: u64, why: String },
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
    ClaimsDone { problem: Option<String> },
    /// A claimed checkout finished: `Ok(None)` off the keyring's list (no period tokens to
    /// issue), `Ok(Some(..))` what became of its period tokens, `Err` the list could not be
    /// changed. `from_claims`: a message of the background claims, which still run.
    CheckoutFinished {
        checkout_id: String,
        result: Result<Option<Finished>, String>,
        from_claims: bool,
    },
    /// What each drive's look found (drive id, its status and redemption).
    PeriodsRedeemed { results: Vec<(String, azcloud_kit::Look)> },
    SettingsSaved {
        result: Result<(), DriveError>,
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
    /// A pending recovery-key lockdown of `drive_id` called off (or why not).
    LockdownCancelled {
        drive_id: String,
        result: Result<(), String>,
    },
    /// A voucher on `drive_id`: the days it added and the period's new end (seconds since
    /// 1970), or why not.
    VoucherRedeemed {
        drive_id: String,
        result: Result<(u32, Option<u64>), String>,
    },
    /// What a sync job did (a pass's progress while it runs).
    Sync(crate::sync_jobs::SyncOutcome),
    /// What "Restore as of..." of `drive_id` as of `as_of` came to, or why not.
    DriveRestored {
        drive_id: String,
        as_of: u64,
        result: Result<crate::restore::Restored, String>,
    },
}

/// A thread's start data: the job, taken out once.
pub(crate) struct JobInit {
    pub job: Option<Job>,
}

/// A thread's answer, taken out once by the write-back.
pub(crate) struct Done {
    pub outcome: Option<Outcome>,
}

/// Sends `outcome` to the UI thread.
fn send(sender: &mut ThreadSender, outcome: Outcome) {
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        crate::on_job_done,
        RefAny::new(Done {
            outcome: Some(outcome),
        }),
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
        return PreviewContent::Message(String::from(
            "No preview: azul could not read this PDF.",
        ));
    };
    let parsed = match ParsedSvg::from_string(svg, SvgParseOptions::create_default()) {
        ResultParsedSvgSvgParseError::Ok(parsed) => parsed,
        ResultParsedSvgSvgParseError::Err(_) => {
            return PreviewContent::Message(String::from(
                "No preview: the PDF's first page could not be drawn.",
            ))
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
        None => PreviewContent::Message(String::from(
            "No preview: the PDF's first page could not be drawn.",
        )),
    }
}

/// The preview of `key`, made on the worker thread.
fn make_preview(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    kind: PreviewKind,
    temp_dir: &std::path::Path,
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
            return PreviewContent::Message(String::from(
                "No preview: the WAV file is too big to fetch for a preview.",
            ));
        }
        return match drive
            .get(key)
            .map_err(|e| e.to_string())
            .and_then(|bytes| preview::wav_samples(&bytes).map_err(String::from))
        {
            Ok(wav) => PreviewContent::Audio(wav),
            Err(why) => PreviewContent::Message(format!("No preview: {why}.")),
        };
    }
    if let Some(reason) = preview::no_preview_reason(kind) {
        return PreviewContent::Message(reason.to_string());
    }
    if kind == PreviewKind::Video {
        if let Some(path) = drive.local_path(key) {
            return PreviewContent::Video(path);
        }
        if !preview::fits_preview(kind, size) {
            return PreviewContent::Message(String::from(
                "No preview: the video is too big to fetch for a preview; open it instead.",
            ));
        }
        return match transfer::download_path(temp_dir, key)
            .ok_or_else(|| DriveError::InvalidKey {
                key: key.to_string(),
                reason: "it has no usable file name",
            })
            .and_then(|dest| {
                transfer::download_to_file(drive, key, size, &dest, transfer::CHUNK)?;
                Ok(dest)
            }) {
            Ok(path) => PreviewContent::Video(path),
            Err(e) => PreviewContent::Message(format!("No preview: {e}")),
        };
    }
    if !preview::fits_preview(kind, size) {
        return PreviewContent::Message(String::from(
            "No preview: the file is too big to fetch for a preview.",
        ));
    }
    let (bytes, truncated) = match preview_bytes(drive, key, size, kind) {
        Ok(read) => read,
        Err(e) => return PreviewContent::Message(format!("No preview: {e}")),
    };
    match kind {
        PreviewKind::Text => match preview::text_preview(&bytes, truncated) {
            Ok(text) => PreviewContent::Text(text),
            Err(why) => PreviewContent::Message(format!("No preview: {why}.")),
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
                        None => PreviewContent::Message(String::from(
                            "No preview: the image could not be prepared.",
                        )),
                    }
                }
                azul::error::ResultRawImageDecodeImageError::Err(_) => PreviewContent::Message(
                    String::from("No preview: azul cannot decode this image."),
                ),
            }
        }
        _ => PreviewContent::Message(String::from("No preview available.")),
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

/// Share > Zip: every file of `items` (folders recursively) into one zip.
fn make_zip(drive: &dyn Drive, items: &[SourceItem], zip_key: &str) -> Result<u64, DriveError> {
    let mut zip = azul::zip::Zip::create();
    for item in items {
        let base = fileops::parent_of(&item.key);
        if item.is_folder {
            zip.add_directory(item.key[base.len()..].to_string());
            for object in storage_ops::list_all(drive, &item.key)? {
                let name = object.key[base.len()..].to_string();
                if object.key.ends_with('/') {
                    zip.add_directory(name);
                } else {
                    let bytes = drive.get(&object.key)?;
                    zip.add_file(name, bytes);
                }
            }
        } else {
            let bytes = drive.get(&item.key)?;
            zip.add_file(item.key[base.len()..].to_string(), bytes);
        }
    }
    let bytes: Vec<u8> = zip.to_bytes().as_slice().to_vec();
    let len = bytes.len() as u64;
    drive.put(zip_key, &bytes)?;
    Ok(len)
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
/// once), then one every [`listing::BATCH_MS`]; the answer is the last batch, `done`. Names and
/// kinds only - the sizes and dates of the rows in view come from [`Job::Stat`]. A set `cancel`
/// (the window went elsewhere) ends it between two entries.
fn scan_dir(
    dir: &Path,
    prefix: &str,
    serial: u64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let entries = match fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(e) => {
            return Outcome::Scanned {
                serial,
                batch: Vec::new(),
                done: true,
                error: Some(e.to_string()),
            }
        }
    };
    let at_root = prefix.is_empty();
    let mut batch = Vec::with_capacity(listing::FIRST_BATCH);
    let mut sent_first = false;
    let mut last = Instant::now();
    for entry in entries {
        if cancel.load(Ordering::Relaxed) {
            return Outcome::Scanned {
                serial,
                batch: Vec::new(),
                done: true,
                error: None,
            };
        }
        let Ok(entry) = entry else {
            continue;
        };
        // A name that is not UTF-8 has no key that could name it.
        let Some(name) = entry.file_name().to_str().map(str::to_string) else {
            continue;
        };
        if !listing::listed_name(&name, at_root) {
            continue;
        }
        let Some(is_folder) = entry_kind(&entry) else {
            continue;
        };
        batch.push(listing::scanned_entry(prefix, &name, is_folder));
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
    Outcome::Scanned {
        serial,
        batch,
        done: true,
        error: None,
    }
}

/// The scan of a bucket's folder: page after page, each page a batch, until the last page or a
/// set `cancel`.
fn scan_bucket(
    drive: &dyn Drive,
    prefix: &str,
    serial: u64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Outcome),
) -> Outcome {
    let mut next: Option<String> = None;
    loop {
        let mut request = ListRequest::folder(prefix).with_max_keys(SCAN_PAGE);
        if let Some(token) = next.take() {
            request = request.with_continuation(token);
        }
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
                    error: Some(crate::problems::describe(&e)),
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
    let still_new = if key.ends_with('/') {
        storage_ops::list_all(drive, key)?
            .iter()
            .all(|o| o.key == key)
    } else {
        drive.head(key)?.size == 0
    };
    if !still_new {
        return Err(DriveError::InvalidKey {
            key: key.to_string(),
            reason: "it is not empty any more; delete it instead",
        });
    }
    if key.ends_with('/') {
        drive.delete_folder(key)
    } else {
        drive.delete(key)
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

/// Seconds between two rounds of the background claims.
const CLAIM_POLL_SECS: u64 = 10;
/// How long the background claims go on in one run (the next start asks again).
const CLAIM_WAIT_SECS: u64 = 3600;

/// A checkout of `tier`, its sign-up sealed to a new claim key; the checkout (its key's secret,
/// its tier, the drive's `name`, its token server) goes on the keyring's list of unfinished
/// checkouts BEFORE its payment page opens - a drive paid after this window stopped waiting, or
/// after AzDrive closed, is claimed with it.
fn start_checkout(
    token_url: &str,
    tier: &str,
    months: u32,
    name: &str,
    keyring: &SharedKeyring,
    via: Option<&PayVia>,
) -> Result<Started, String> {
    let transport = AzulTransport::new(USER_AGENT);
    let claim = ClaimKey::generate().map_err(|e| e.to_string())?;
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let (checkout, answer) = match via {
        None => (
            server
                .checkout(tier, months, azcloud_kit::token::DEFAULT_METHOD, &claim)
                .map_err(|e| e.to_string())?,
            None,
        ),
        Some(via) => {
            let through = CheckoutVia {
                provider: via.choice.provider.spec.id,
                method: via.choice.method.method.as_str(),
                surface: via.surface.as_str(),
                vat_country: &via.country,
                withdrawal_consent: via.consent,
            };
            let (checkout, answer) = server
                .checkout_via(tier, months, &through, &claim)
                .map_err(|e| e.to_string())?;
            (checkout, Some(answer))
        }
    };
    let kept = PendingCheckout::new(&checkout.checkout_id, &claim, tier, server.base(), name);
    pending::add(keyring, &kept).map_err(|e| {
        format!("the keyring did not keep the checkout's claim key ({e}), so it was not opened")
    })?;
    let created = match (via, answer) {
        (Some(via), Some(answer)) => match Created::parse(&answer, &via.choice, &via.look) {
            Ok(created) => Some(created),
            Err(refused) => {
                // Nothing of it is shown: nobody may pay it.
                let _ = server.abandon_checkout(&checkout.checkout_id);
                let _ = pending::remove(keyring, &checkout.checkout_id);
                return Err(refused.to_string());
            }
        },
        _ => None,
    };
    Ok(Started {
        checkout,
        kept,
        created,
    })
}

/// The checkout `checkout_id` on the surface `kind`, checked for `choice`.
fn switch_surface(
    token_url: &str,
    checkout_id: &str,
    kind: SurfaceKind,
    choice: &Choice,
    look: &Look,
) -> Result<azul_pay::Surface, String> {
    let transport = AzulTransport::new(USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let answer = server
        .checkout_surface(checkout_id, kind.as_str())
        .map_err(|e| e.to_string())?;
    let surface = answer
        .get("surface")
        .ok_or_else(|| String::from("the answer has no surface"))?;
    azul_pay::Surface::parse(surface, choice, look).map_err(|e| e.to_string())
}

/// The checkout `checkout_id` abandoned at the token server and off the keyring's list.
fn abandon(token_url: &str, checkout_id: &str, keyring: &SharedKeyring) -> Result<(), String> {
    let transport = AzulTransport::new(USER_AGENT);
    // Off the list first: whatever the token server says, this app never claims it.
    let removed = pending::remove(keyring, checkout_id).map(|_| ());
    let told = TokenServer::new(token_url, &transport)
        .and_then(|server| server.abandon_checkout(checkout_id))
        .map_err(|e| e.to_string());
    told.and(removed.map_err(|e| e.to_string()))
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
    let transport = AzulTransport::new(USER_AGENT);
    let server = match TokenServer::new(token_url, &transport) {
        Ok(server) => server,
        Err(e) => {
            return Outcome::PaymentEnded {
                serial,
                why: e.to_string(),
            }
        }
    };
    let started = Instant::now();
    let mut last_problem = String::new();
    loop {
        // A few seconds in short steps: Stop waiting is heard at once.
        for _ in 0..(PAYMENT_POLL_SECS * 4) {
            if cancel.load(Ordering::SeqCst) {
                // "Stop waiting" said why, and the background claims take over.
                return Outcome::PaymentEnded {
                    serial,
                    why: String::new(),
                };
            }
            std::thread::sleep(Duration::from_millis(250));
        }
        if started.elapsed() > Duration::from_secs(PAYMENT_WAIT_SECS) {
            let problem = if last_problem.is_empty() {
                String::new()
            } else {
                format!(" (last: {last_problem})")
            };
            return Outcome::PaymentEnded {
                serial,
                why: format!(
                    "No payment arrived within an hour{problem}. The checkout is kept: a payment \
                     made later still brings the drive, at the next start at the latest."
                ),
            };
        }
        match pending::poll(&server, keyring, checkout) {
            Polled::Pending => {}
            Polled::Kept(why) => last_problem = why,
            Polled::Claimed(claimed) => {
                return Outcome::Claimed {
                    serial: Some(serial),
                    checkout: checkout.clone(),
                    claimed,
                }
            }
            Polled::Dropped(why) => {
                return Outcome::PaymentEnded {
                    serial,
                    why: format!("The checkout ended: {why}."),
                }
            }
            Polled::Settled => {
                return Outcome::PaymentEnded {
                    serial,
                    why: String::from("Another AzDrive window finished this checkout."),
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
fn claim_pending(
    keyring: &SharedKeyring,
    token_url: Option<&str>,
    store: &PeriodTokenStore,
    sender: &mut ThreadSender,
) -> Outcome {
    let transport = AzulTransport::new(USER_AGENT);
    let started = Instant::now();
    let mut reported: Vec<String> = Vec::new();
    loop {
        let open: Vec<PendingCheckout> = match pending::list(keyring) {
            Ok(checkouts) => checkouts
                .into_iter()
                .filter(|c| c.period.is_some() || !reported.contains(&c.checkout_id))
                .collect(),
            Err(e) => {
                // A system without a keyring kept no checkout either: nothing to say then.
                let no_keyring = matches!(
                    e.root(),
                    CloudError::Keyring(azul_storage::keyring::KeyringError::Unavailable)
                );
                return Outcome::ClaimsDone {
                    problem: (!no_keyring).then(|| {
                        format!("The unfinished checkouts could not be read from the keyring: {e}")
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
                let finished = pending::finish(&server, keyring, store, &checkout, owed);
                // A try that failed is tried again next round, quietly.
                if !matches!(finished, Finished::Kept(_)) {
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
            match pending::poll(&server, keyring, &checkout) {
                Polled::Claimed(claimed) => {
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
                Polled::Dropped(why) => send(
                    sender,
                    Outcome::CheckoutDropped {
                        checkout_id: checkout.checkout_id,
                        why,
                    },
                ),
                Polled::Pending | Polled::Kept(_) | Polled::Settled => {}
            }
        }
        if asked == 0 {
            return Outcome::ClaimsDone {
                problem: Some(String::from(
                    "Unfinished checkouts wait in the keyring, but no Azlin token server is set \
                     to ask about them.",
                )),
            };
        }
        if started.elapsed() > Duration::from_secs(CLAIM_WAIT_SECS) {
            return Outcome::ClaimsDone { problem: None };
        }
        std::thread::sleep(Duration::from_secs(CLAIM_POLL_SECS));
    }
}

/// A look at each of `drives` (its id and token server): its status - the period, a pending
/// recovery-key lockdown - and, when its period nears its end, its next month from a period
/// token kept in `store` ([`look_at_drive`]: under the drive's keyring lock, with its newest
/// drive token).
fn redeem_periods(
    keyring: &SharedKeyring,
    store: &PeriodTokenStore,
    drives: &[(String, String)],
) -> Outcome {
    let transport = AzulTransport::new(USER_AGENT);
    let now = crate::actions::now_secs();
    let results = drives
        .iter()
        .filter_map(|(drive_id, token_url)| {
            let server = TokenServer::new(token_url, &transport).ok()?;
            Some((
                drive_id.clone(),
                look_at_drive(&server, keyring, store, drive_id, now),
            ))
        })
        .collect();
    Outcome::PeriodsRedeemed { results }
}

/// The claimed `checkout`'s drive `drive_id` is in the drives file (AZDRIVE-INTEGRATION §4):
/// without a `grant` its checkout leaves the keyring's list; with one the list keeps its issue
/// key with its claim secret ([`pending::claimed`]) and its period tokens are issued at
/// `token_url` and kept in `store` - only then does it leave the list ([`pending::finish`]).
fn finish_checkout(
    keyring: &SharedKeyring,
    store: &PeriodTokenStore,
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
    let owed = PendingTokens {
        drive_id: drive_id.to_string(),
        months: grant.months,
        issue_key: grant.issue_key.clone(),
    };
    let transport = AzulTransport::new(USER_AGENT);
    let finished = match TokenServer::new(token_url, &transport) {
        Ok(server) => pending::finish(&server, keyring, store, checkout, &owed),
        Err(e) => Finished::Kept(e.to_string()),
    };
    outcome(Ok(Some(finished)))
}

fn run_job(job: Job, sender: &mut ThreadSender) -> Outcome {
    match job {
        Job::Scan {
            drive,
            dir,
            prefix,
            serial,
            cancel,
        } => {
            let mut emit = |outcome: Outcome| send(sender, outcome);
            match dir {
                Some(dir) => scan_dir(&dir, &prefix, serial, &cancel, &mut emit),
                None => scan_bucket(&*drive, &prefix, serial, &cancel, &mut emit),
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
        Job::Folders { drive, dir, node } => Outcome::Folders {
            result: tree_folders(&*drive, dir.as_deref(), &node),
            node,
        },
        Job::Plan {
            id,
            source,
            items,
            target,
            target_prefix,
            same_drive,
            kind,
        } => Outcome::Planned {
            id,
            result: fileops::plan_transfer(
                &*source,
                &items,
                &*target,
                &target_prefix,
                same_drive,
                kind,
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
            result: fileops::delete_items(&*drive, &items, stamp.as_deref()),
        },
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
                match storage_ops::exists(&*drive, &key) {
                    Ok(true) => Err(DriveError::InvalidKey {
                        key: key.clone(),
                        reason: "something has this name already",
                    }),
                    Ok(false) => drive.put(&key, &[]),
                    Err(e) => Err(e),
                }
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
                pairs
                    .iter()
                    .try_for_each(|(now, before)| drive.rename(now, before))
                    .and_then(|()| match &remove {
                        Some(key) => undo_create(&*drive, key),
                        None => Ok(()),
                    })
            };
            Outcome::Undone { result }
        }
        Job::Preview {
            drive,
            key,
            size,
            kind,
            temp_dir,
        } => Outcome::Previewed {
            content: make_preview(&*drive, &key, size, kind, &temp_dir),
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
        } => Outcome::Zipped {
            result: make_zip(&*drive, &items, &zip_key),
            zip_key,
        },
        Job::Open {
            drive,
            key,
            size,
            folder,
        } => {
            let result = match drive.local_path(&key) {
                // A local file opens where it is.
                Some(path) => Ok(path),
                None => transfer::download_path(&folder, &key)
                    .ok_or_else(|| DriveError::InvalidKey {
                        key: key.clone(),
                        reason: "it has no usable file name",
                    })
                    .and_then(|dest| {
                        transfer::download_to_file(&*drive, &key, size, &dest, transfer::CHUNK)?;
                        Ok(dest)
                    }),
            };
            Outcome::Opened { key, result }
        }
        Job::Test {
            serial,
            entry,
            secret,
        } => {
            // ONE listing call, at most one entry: does the source answer with these settings?
            let result = entry
                .open_with_secret(secret.as_deref(), Box::new(AzulTransport::new(USER_AGENT)))
                .and_then(|drive| drive.list(&ListRequest::folder("").with_max_keys(1)))
                .map(|page| {
                    if page.folders.is_empty() && page.objects.is_empty() {
                        String::from("Connection OK: the source answered; it is empty.")
                    } else {
                        String::from("Connection OK: the source answered and lists its files.")
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
                &AzulTransport::new(USER_AGENT),
                &plan.token_url,
                &plan.client,
                &code,
                &code_verifier,
                &redirect_uri,
            )
            .map_err(|e| e.to_string());
            Outcome::SignedIn { serial, result }
        }
        Job::Tiers { serial, token_url } => {
            let transport = AzulTransport::new(USER_AGENT);
            let result = TokenServer::new(&token_url, &transport)
                .and_then(|server| server.tiers())
                .map_err(|e| e.to_string());
            Outcome::Tiers { serial, result }
        }
        Job::CreateTestDrive {
            serial,
            token_url,
            name,
            tier,
            keyring,
        } => {
            let transport = AzulTransport::new(USER_AGENT);
            let result = TokenServer::new(&token_url, &transport)
                .and_then(|server| server.create_dev_drive(&name, &tier))
                .map_err(|e| {
                    if e.is_checkout_only() {
                        String::from(
                            "This token server sells drives through a checkout only: it makes \
                             no test drives. Use Buy.",
                        )
                    } else {
                        e.to_string()
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
            name,
            keyring,
            via,
        } => Outcome::CheckoutStarted {
            serial,
            result: start_checkout(&token_url, &tier, months, &name, &keyring, via.as_deref()),
        },
        Job::Options {
            serial,
            token_url,
            tier,
            months,
            country,
            currency,
        } => {
            let transport = AzulTransport::new(USER_AGENT);
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
                .map_err(|e| e.to_string());
            Outcome::Options { serial, result }
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
        } => claim_pending(&keyring, token_url.as_deref(), &store, sender),
        Job::FinishCheckout {
            keyring,
            store,
            checkout,
            drive_id,
            grant,
            token_url,
        } => finish_checkout(
            &keyring,
            &store,
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
        Job::SaveSettings { drive, text } => Outcome::SettingsSaved {
            result: drive.put(crate::SETTINGS_KEY, text.as_bytes()),
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
        Job::CancelLockdown {
            keyring,
            drive_id,
            token_url,
        } => Outcome::LockdownCancelled {
            result: cancel_lockdown(&keyring, &drive_id, &token_url),
            drive_id,
        },
        Job::RedeemVoucher {
            serial,
            token_url,
            code,
            tier,
            drive,
            keyring,
        } => redeem_voucher(serial, &token_url, &code, &tier, drive, &keyring),
        Job::Sync(job) => {
            let mut emit = |outcome: Outcome| send(sender, outcome);
            crate::sync_jobs::run(job, &mut emit)
        }
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
    }
}

/// A pending recovery-key lockdown of `drive_id` called off: a grant, so under the drive's
/// keyring lock with its newest drive token. None pending any more (409) is done too.
fn cancel_lockdown(
    keyring: &SharedKeyring,
    drive_id: &str,
    token_url: &str,
) -> Result<(), String> {
    let transport = AzulTransport::new(USER_AGENT);
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let answer = keyring
        .with_drive_token(drive_id, |token| server.lockdown_cancel(drive_id, token))
        .map_err(|e| e.to_string())?;
    match answer {
        Ok(_) => Ok(()),
        Err(TokenError::Refused { code, .. }) if code == "no_pending_lockdown" => Ok(()),
        Err(e) => Err(e.to_string()),
    }
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
    let transport = AzulTransport::new(USER_AGENT);
    let server = match TokenServer::new(token_url, &transport) {
        Ok(server) => server,
        Err(e) => {
            return match drive {
                Some(drive_id) => Outcome::VoucherRedeemed {
                    drive_id,
                    result: Err(e.to_string()),
                },
                None => Outcome::Bought {
                    serial,
                    result: Err(e.to_string()),
                },
            }
        }
    };
    if let Some(drive_id) = drive {
        let redeemed = keyring
            .with_drive_token(&drive_id, |token| {
                server.redeem_voucher(code, Some((drive_id.as_str(), token)), "")
            })
            .map_err(|e| e.to_string())
            .and_then(|answer| answer.map_err(|e| e.to_string()));
        let result = match redeemed {
            Ok(VoucherRedeemed::Extended {
                days_added,
                period_until,
            }) => Ok((days_added, period_until)),
            Ok(VoucherRedeemed::NewDrive(_)) => Err(String::from(
                "The token server made a new drive instead of extending this one.",
            )),
            Err(why) => Err(why),
        };
        return Outcome::VoucherRedeemed { drive_id, result };
    }
    let result = match server.redeem_voucher(code, None, tier) {
        Ok(VoucherRedeemed::NewDrive(bundle)) => {
            let bundle = *bundle;
            println!("AZDRIVE_VOUCHER new {}", bundle.drive_id());
            Ok(match keyring.keep_new_drive(&bundle) {
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
            })
        }
        Ok(VoucherRedeemed::Extended { .. }) => Err(String::from(
            "The token server answered with days for a drive, not with a new drive.",
        )),
        Err(e) => Err(e.to_string()),
    };
    Outcome::Bought { serial, result }
}

/// Runs on a worker thread: the blocking storage call, then its answer to
/// the UI thread (a transfer sends its progress on the way).
pub(crate) extern "C" fn job_thread(
    mut init: RefAny,
    mut sender: ThreadSender,
    _receiver: ThreadReceiver,
) {
    let Some(job) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut init| init.job.take())
    else {
        return;
    };
    let outcome = run_job(job, &mut sender);
    send(&mut sender, outcome);
}

#[cfg(test)]
mod tests {
    use azul_storage::testing::TempDir;

    use super::*;

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
        assert!(all.iter().all(|e| !e.known && e.size.is_none()));
        assert!(all.iter().any(|e| e.key == "docs/sub-0/"));
        assert!(all.iter().any(|e| e.key == "docs/file-00000.txt"));
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
}

