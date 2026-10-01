//! The blocking storage work of AzDrive, one [`Job`] per azul `Thread`: a
//! listing, a transfer plan, a transfer run (with progress messages while it
//! copies), a delete into the trash or for good, a rename, a new folder or
//! file, an undo, a preview, a folder's size, an object's metadata, a zip,
//! the settings file, a download to open. Every answer comes back to the UI
//! thread as an [`Outcome`] through the thread's write-back. No callback
//! ever waits on a drive.

use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Instant,
};

use azul::{
    image::{ImageRef, RawImage},
    prelude::*,
    vec::U8VecRef,
};
use azul_storage::{
    azul_transport::AzulTransport, ops as storage_ops, transfer, ByteRange, Credentials,
    Drive, DriveError, ListPage, ListRequest, LocalDrive, S3Config, S3Drive,
};

use crate::{
    fileops::{self, Plan, Progress, SourceItem, TransferKind, TransferReport},
    preview::{self, PreviewKind},
    TreeKey, USER_AGENT,
};

/// Why a listing was asked for.
pub(crate) enum ListPurpose {
    /// The open folder's rows.
    Content { serial: u64, append: bool },
    /// A tree node's folders.
    Tree { node: TreeKey },
}

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
    List {
        drive: Arc<dyn Drive>,
        request: ListRequest,
        purpose: ListPurpose,
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
    Listed {
        purpose: ListPurpose,
        result: Result<ListPage, DriveError>,
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

/// The preview of `key`, made on the worker thread.
fn make_preview(
    drive: &dyn Drive,
    key: &str,
    size: Option<u64>,
    kind: PreviewKind,
    temp_dir: &std::path::Path,
) -> PreviewContent {
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
        Job::List {
            drive,
            request,
            purpose,
        } => Outcome::Listed {
            purpose,
            result: drive.list(&request),
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

