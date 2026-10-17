//! The work off the UI thread. Each job runs on an azul `Thread` and hands its results back by
//! write-backs, a batch at a time, so no callback ever waits on the disk, a decoder or a big
//! picture:
//!
//! - THE LIBRARY SCAN of a library's folders: the files found, each once (`library::find_files`),
//!   a song's tags and whether it has a cover (azul's `AudioFileDecoder`, which streams the file -
//!   never reads it whole), a video's length (its `mvhd` box, read in place);
//! - THE PICTURES: a picture's thumbnail, a song's cover, the picture viewer's window-sized copy -
//!   decoded and scaled down on the worker (`RawImage::decode_image_bytes_any`,
//!   `RawImage::thumbnail`); the `ImageRef` is made there too (it is shared across threads).
//!
//! A worker that is told to stop (the window closes, `remove_thread`) stops between two files.

use std::path::PathBuf;

use azul::{
    audio::AudioFileDecoder,
    callbacks::WriteBackCallbackType,
    error::ResultRawImageDecodeImageError,
    image::{ImageRef, RawImage},
    option::OptionThreadSendMsg,
    prelude::*,
    str::String as AzString,
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSendMsg, ThreadSender, ThreadWriteBackMsg},
    vec::U8VecRef,
};

use crate::library::{self, Item, Shelf};

/// How many items a scan hands back at once.
const BATCH: usize = 64;

/// Whether the UI asked the worker to stop.
fn told_to_stop(receiver: &mut ThreadReceiver) -> bool {
    matches!(
        receiver.recv(),
        OptionThreadSendMsg::Some(ThreadSendMsg::TerminateThread)
    )
}

// ==== The library scan ====

struct ScanInit {
    shelf: Shelf,
    /// The library's folders.
    roots: Vec<PathBuf>,
    /// The folders inside them that another library reads (recorded TV inside the videos).
    skip: Vec<PathBuf>,
    on_batch: WriteBackCallbackType,
}

/// What a scan hands back: the next items of `shelf`.
pub struct ScanBatch {
    pub shelf: Shelf,
    pub items: Vec<Item>,
    /// The first batch of this scan (the items so far start over).
    pub first: bool,
    /// The last batch: the scan is over.
    pub done: bool,
    /// The scan stopped at the file limit.
    pub cut: bool,
    /// None of the library's folders is there.
    pub missing: bool,
}

/// Scans the folders `roots` for `shelf` on a Thread; `on_batch(app, ScanBatch, info)` gets the
/// items on the UI thread, a batch at a time.
pub fn spawn_scan(
    info: &mut CallbackInfo,
    app: &RefAny,
    shelf: Shelf,
    roots: Vec<PathBuf>,
    skip: Vec<PathBuf>,
    on_batch: WriteBackCallbackType,
) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(
            RefAny::new(ScanInit {
                shelf,
                roots,
                skip,
                on_batch,
            }),
            app.clone(),
            scan_thread,
        ),
    );
}

/// A song's tags, its length and whether it has a cover (the file is streamed, not read).
fn read_tags(item: &mut Item) {
    let decoder = AudioFileDecoder::open(AzString::from(item.path.as_str()));
    if !decoder.is_open() {
        return;
    }
    let info = decoder.info();
    let title = info.title.as_str().trim();
    if !title.is_empty() {
        item.title = title.to_string();
    }
    item.artist = info.artist.as_str().trim().to_string();
    item.album = info.album.as_str().trim().to_string();
    item.album_artist = info.album_artist.as_str().trim().to_string();
    item.genre = info.genre.as_str().trim().to_string();
    item.year = info.date.as_str().trim().chars().take(4).collect();
    item.track_no = info.track_number;
    item.disc_no = info.disc_number;
    item.duration_s = info.duration_s;
    item.has_cover = !info.cover.as_slice().is_empty();
}

extern "C" fn scan_thread(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some((shelf, roots, skip, on_batch)) = init
        .downcast_ref::<ScanInit>()
        .map(|i| (i.shelf, i.roots.clone(), i.skip.clone(), i.on_batch))
    else {
        return;
    };
    let mut send = |batch: ScanBatch| -> bool {
        sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            on_batch,
            RefAny::new(batch),
        )))
    };
    if !library::any_folder_there(&roots) {
        let _ = send(ScanBatch {
            shelf,
            items: Vec::new(),
            first: true,
            done: true,
            cut: false,
            missing: true,
        });
        return;
    }
    let (paths, cut) = library::find_files(shelf, &roots, &skip);
    let mut items = Vec::with_capacity(BATCH);
    let mut first = true;
    for path in paths {
        if told_to_stop(&mut receiver) {
            return;
        }
        let mut item = Item::from_path(&path);
        match shelf {
            Shelf::Music => read_tags(&mut item),
            Shelf::Videos | Shelf::Tv => {
                item.duration_s = library::mp4_duration_s(&path).unwrap_or(0.0);
            }
            Shelf::Pictures => {}
        }
        items.push(item);
        if items.len() >= BATCH {
            let batch = ScanBatch {
                shelf,
                items: std::mem::take(&mut items),
                first,
                done: false,
                cut: false,
                missing: false,
            };
            first = false;
            if !send(batch) {
                return;
            }
        }
    }
    let _ = send(ScanBatch {
        shelf,
        items,
        first,
        done: true,
        cut,
        missing: false,
    });
}

// ==== The pictures ====

/// What a picture is made from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArtSource {
    /// A picture file.
    Picture(String),
    /// The cover inside a song's file.
    Cover(String),
}

/// One picture to make: its key in the app's cache, where it comes from, how big at most
/// (device px, both sides).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArtJob {
    pub key: String,
    pub source: ArtSource,
    pub max_px: u32,
}

/// A picture made: its key, the image (`None`: it could not be read) and its size in pixels.
pub struct ArtDone {
    pub key: String,
    pub image: Option<(ImageRef, f32, f32)>,
    /// The last of its job list.
    pub last: bool,
}

struct ArtInit {
    jobs: Vec<ArtJob>,
    on_done: WriteBackCallbackType,
}

/// Makes the pictures of `jobs` (in order) on a Thread; `on_done(app, ArtDone, info)` gets each
/// as soon as it is made.
pub fn spawn_art(
    info: &mut CallbackInfo,
    app: &RefAny,
    jobs: Vec<ArtJob>,
    on_done: WriteBackCallbackType,
) {
    if jobs.is_empty() {
        return;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(ArtInit { jobs, on_done }), app.clone(), art_thread),
    );
}

/// The encoded bytes of a picture: the file, or the cover in a song.
fn art_bytes(source: &ArtSource) -> Option<Vec<u8>> {
    match source {
        ArtSource::Picture(path) => std::fs::read(path).ok(),
        ArtSource::Cover(path) => {
            let decoder = AudioFileDecoder::open(AzString::from(path.as_str()));
            if !decoder.is_open() {
                return None;
            }
            let cover = decoder.info().cover;
            let bytes: &[u8] = cover.as_slice();
            (!bytes.is_empty()).then(|| bytes.to_vec())
        }
    }
}

/// `bytes` decoded and scaled to fit `max_px` (aspect kept), as an image and its size.
#[allow(clippy::cast_precision_loss)]
fn make_image(bytes: &[u8], max_px: u32) -> Option<(ImageRef, f32, f32)> {
    let image = match RawImage::decode_image_bytes_any(U8VecRef::from(bytes)) {
        ResultRawImageDecodeImageError::Ok(image) => image,
        ResultRawImageDecodeImageError::Err(_) => return None,
    };
    let small = image.thumbnail(max_px, max_px).into_option()?;
    let (w, h) = (small.width as f32, small.height as f32);
    let image = ImageRef::create_rawimage(small).into_option()?;
    Some((image, w, h))
}

extern "C" fn art_thread(mut init: RefAny, mut sender: ThreadSender, mut receiver: ThreadReceiver) {
    let Some((jobs, on_done)) = init.downcast_mut::<ArtInit>().map(|mut i| {
        (std::mem::take(&mut i.jobs), i.on_done)
    }) else {
        return;
    };
    let count = jobs.len();
    for (n, job) in jobs.into_iter().enumerate() {
        if told_to_stop(&mut receiver) {
            return;
        }
        let image = art_bytes(&job.source).and_then(|bytes| make_image(&bytes, job.max_px));
        let sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            on_done,
            RefAny::new(ArtDone {
                key: job.key,
                image,
                last: n + 1 == count,
            }),
        )));
        if !sent {
            return;
        }
    }
}
