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
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::{Duration, Instant, UNIX_EPOCH},
};

use azcloud_kit::{
    pending::{self, Claimed, Polled},
    Checkout, ClaimKey, CloudError, DriveBundle, PendingCheckout, SharedKeyring, Tiers,
    TokenServer,
};
use azul::{
    image::{ImageRef, RawImage},
    prelude::*,
    vec::U8VecRef,
};
use azul_storage::{
    azul_transport::AzulTransport, config::DriveEntry, ops as storage_ops, transfer, ByteRange,
    Drive, DriveError, ListRequest, LocalDrive,
};

use crate::{
    browse::{self, Entry},
    fileops::{self, Plan, Progress, SourceItem, TransferKind, TransferReport},
    listing::{self, Stat},
    preview::{self, PreviewKind},
    TreeKey, USER_AGENT,
};

/// The keys a bucket's listing asks for per page while a scan streams it.
const SCAN_PAGE: u32 = 1000;

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
    /// opens.
    Checkout {
        serial: u64,
        token_url: String,
        tier: String,
        months: u32,
        name: String,
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
    /// (each at its own token server, else `token_url`) until none is left or an hour is gone.
    Claims {
        keyring: SharedKeyring,
        token_url: Option<String>,
    },
    /// A claimed checkout taken off the keyring's list (its drive is in the drives file).
    ForgetCheckout {
        keyring: SharedKeyring,
        checkout_id: String,
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
    /// A checkout to pay in the browser, on the keyring's list of unfinished checkouts (or why
    /// there is none).
    CheckoutStarted {
        serial: u64,
        result: Result<(Checkout, PendingCheckout), String>,
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
    /// A claimed checkout off the keyring's list (`false`: another window took it off).
    CheckoutForgotten {
        checkout_id: String,
        result: Result<bool, String>,
    },
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
                return Outcome::Scanned {
                    serial,
                    batch: Vec::new(),
                    done: true,
                    error: Some(e.to_string()),
                }
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
) -> Result<(Checkout, PendingCheckout), String> {
    let transport = AzulTransport::new(USER_AGENT);
    let claim = ClaimKey::generate().map_err(|e| e.to_string())?;
    let server = TokenServer::new(token_url, &transport).map_err(|e| e.to_string())?;
    let checkout = server
        .checkout(tier, months, azcloud_kit::token::DEFAULT_METHOD, &claim)
        .map_err(|e| e.to_string())?;
    let kept = PendingCheckout::new(&checkout.checkout_id, &claim, tier, server.base(), name);
    pending::add(keyring, &kept).map_err(|e| {
        format!("the keyring did not keep the checkout's claim key ({e}), so it was not opened")
    })?;
    Ok((checkout, kept))
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
/// reported again while the window takes its checkout off the list.
fn claim_pending(
    keyring: &SharedKeyring,
    token_url: Option<&str>,
    sender: &mut ThreadSender,
) -> Outcome {
    let transport = AzulTransport::new(USER_AGENT);
    let started = Instant::now();
    let mut reported: Vec<String> = Vec::new();
    loop {
        let open: Vec<PendingCheckout> = match pending::list(keyring) {
            Ok(checkouts) => checkouts
                .into_iter()
                .filter(|c| !reported.contains(&c.checkout_id))
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
        } => Outcome::CheckoutStarted {
            serial,
            result: start_checkout(&token_url, &tier, months, &name, &keyring),
        },
        Job::AwaitPayment {
            serial,
            checkout,
            token_url,
            keyring,
            cancel,
        } => await_payment(serial, &checkout, &token_url, &keyring, &cancel),
        Job::Claims { keyring, token_url } => {
            claim_pending(&keyring, token_url.as_deref(), sender)
        }
        Job::ForgetCheckout {
            keyring,
            checkout_id,
        } => Outcome::CheckoutForgotten {
            result: pending::remove(&keyring, &checkout_id).map_err(|e| e.to_string()),
            checkout_id,
        },
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
    }
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

