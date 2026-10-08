//! The blocking storage work of AzDrive, one [`Job`] per azul `Thread`: the
//! open folder's scan (its batches streamed while it reads), the stat of the
//! rows in view, the item counts of folders, a tree node's folders, a
//! transfer plan, a transfer run (with progress messages while it copies), a
//! delete into the trash or for good, a rename, a new folder or file, an
//! undo, a preview, a folder's size, an object's metadata, a zip, the
//! settings file, a download to open. Every answer comes back to the UI
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

use azul::{
    image::{ImageRef, RawImage},
    prelude::*,
    vec::U8VecRef,
};
use azul_storage::{
    azul_transport::AzulTransport, ops as storage_ops, transfer, ByteRange, Credentials,
    Drive, DriveError, ListRequest, LocalDrive, S3Config, S3Drive,
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
    Test {
        serial: u64,
        config: S3Config,
        credentials: Credentials,
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
            stats: keys.iter().filter_map(|key| stat_of(&root, key)).collect(),
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
            config,
            credentials,
        } => {
            // ONE listing call, at most one entry: does the bucket answer to these keys?
            let result = S3Drive::new(
                config,
                credentials,
                Box::new(AzulTransport::new(USER_AGENT)),
            )
            .and_then(|drive| drive.list(&ListRequest::folder("").with_max_keys(1)))
            .map(|page| {
                if page.folders.is_empty() && page.objects.is_empty() {
                    String::from("Connection OK: the bucket answered; it is empty.")
                } else {
                    String::from("Connection OK: the bucket answered and lists its files.")
                }
            });
            Outcome::Tested { serial, result }
        }
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

