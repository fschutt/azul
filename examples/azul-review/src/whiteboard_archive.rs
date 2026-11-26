//! The whiteboard's file on azul's codecs: the entries of `whiteboard_store` as a zip, and the PNGs of
//! its cutouts through azul's PNG codec. Thin glue: what a file holds, how it is read and what is
//! refused is `whiteboard_store`'s, tested on its own.
//!
//! A save is built on the UI thread (the entries of the store: the manifest and the pictures' PNGs, as they
//! are kept), and made into the zip and written on a Thread of its own ([`spawn_save`]: atomic, one writer -
//! the owner's `SaveGate`), so that a full board never holds the window up. A load decodes every picture,
//! which takes a while for a full board: [`spawn_load`] does it on a Thread of its own and hands the board
//! back.

use std::path::{Path, PathBuf};

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    error::{ResultRawImageDecodeImageError, ResultU8VecEncodeImageError},
    image::{RawImage, RawImageData, RawImageFormat},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
    vec::U8VecRef,
    zip::Zip,
};

use crate::{
    whiteboard::{BoardData, Limits},
    whiteboard_azboard::Meta,
    whiteboard_export::{self as export, ExportRequest, Exported},
    whiteboard_store::{self as store, DecodedPng, Entry, ImageCodec, Loaded, StoreError},
};

/// The largest board file that is looked at, bytes: more than any board of a workspace is.
pub const MAX_ARCHIVE_BYTES: usize = 1 << 30;

/// Straight RGBA8 rows of a decoded image (`None` when its pixels are not 8-bit).
fn straight_rgba(image: &RawImage) -> Option<Vec<u8>> {
    let n = image.width.checked_mul(image.height)?;
    let RawImageData::U8(bytes) = &image.pixels else {
        return None;
    };
    let src: &[u8] = bytes.as_ref();
    let mut out = vec![0_u8; n.checked_mul(4)?];
    let channels = match image.data_format {
        RawImageFormat::R8 => 1,
        RawImageFormat::RG8 => 2,
        RawImageFormat::RGB8 | RawImageFormat::BGR8 => 3,
        RawImageFormat::RGBA8 | RawImageFormat::BGRA8 => 4,
        _ => return None,
    };
    if src.len() < n * channels {
        return None;
    }
    for i in 0..n {
        let s = &src[i * channels..(i + 1) * channels];
        out[i * 4..i * 4 + 4].copy_from_slice(&match image.data_format {
            RawImageFormat::R8 => [s[0], s[0], s[0], 255],
            RawImageFormat::RG8 => [s[0], s[0], s[0], s[1]],
            RawImageFormat::RGB8 => [s[0], s[1], s[2], 255],
            RawImageFormat::BGR8 => [s[2], s[1], s[0], 255],
            RawImageFormat::RGBA8 => [s[0], s[1], s[2], s[3]],
            _ => [s[2], s[1], s[0], s[3]],
        });
    }
    if image.premultiplied_alpha {
        for px in out.chunks_exact_mut(4) {
            let a = u32::from(px[3]);
            if a > 0 && a < 255 {
                for c in &mut px[..3] {
                    *c = ((u32::from(*c) * 255 + a / 2) / a).min(255) as u8;
                }
            }
        }
    }
    Some(out)
}

/// Reads a PNG (or any image azul reads) into straight RGBA8.
///
/// # Errors
/// A sentence for what is wrong with the bytes.
pub fn decode_png(bytes: &[u8]) -> Result<DecodedPng, String> {
    match RawImage::decode_image_bytes_any(U8VecRef::from(bytes)) {
        ResultRawImageDecodeImageError::Ok(image) => {
            let rgba = straight_rgba(&image).ok_or_else(|| "its pixels are not of a kind the board reads".to_string())?;
            Ok(DecodedPng {
                width: u32::try_from(image.width).map_err(|_| "it is too wide".to_string())?,
                height: u32::try_from(image.height).map_err(|_| "it is too tall".to_string())?,
                rgba,
            })
        }
        ResultRawImageDecodeImageError::Err(e) => Err(format!("{e:?}")),
    }
}

/// The PNG of `w` x `h` straight RGBA8 pixels.
///
/// # Errors
/// A sentence for why the picture could not be encoded.
pub fn encode_png(w: u32, h: u32, straight_rgba: &[u8]) -> Result<Vec<u8>, String> {
    match RawImage::create_rgba8(w, h, straight_rgba.to_vec(), false).encode_png() {
        ResultU8VecEncodeImageError::Ok(bytes) => Ok(bytes.as_ref().to_vec()),
        ResultU8VecEncodeImageError::Err(e) => Err(format!("the picture could not be written ({e:?})")),
    }
}

/// azul's PNG decoder as the store's [`ImageCodec`].
#[derive(Debug, Clone, Copy, Default)]
pub struct AzulCodec;

impl ImageCodec for AzulCodec {
    fn decode_png(&self, png: &[u8]) -> Result<DecodedPng, String> {
        decode_png(png)
    }
}

/// How hard the entries are deflated: lightly (level 1). The pictures are PNGs already and the manifest is a
/// few hundred kilobytes, so a save built on the window's thread costs little more than a copy. (Level 0, "stored",
/// would be the exact fit, but azul's zip writer cannot write it yet: `ZipWriteConfig::store()` hands the backend a
/// compression level it refuses for a stored entry - the documented bug `autotest_bug_store_config_cannot_write_file_entries`
/// in `layout/src/zip.rs`; `to_bytes_with_level(0)` answers with nothing.)
const ZIP_LEVEL: u8 = 1;

/// The entries as the bytes of one zip.
///
/// # Errors
/// The zip could not be made (azul answers with nothing).
pub fn pack(entries: &[Entry]) -> Result<Vec<u8>, String> {
    let mut zip = Zip::create();
    for e in entries {
        zip.add_file(e.name.as_str(), e.bytes.clone());
    }
    let bytes = zip.to_bytes_with_level(ZIP_LEVEL);
    if bytes.as_ref().is_empty() {
        return Err("the archive could not be made".to_string());
    }
    Ok(bytes.as_ref().to_vec())
}

/// The file's name and bytes for a board of `data`: what a save hands to the file job.
///
/// # Errors
/// The zip could not be made.
pub fn archive(data: &BoardData, workspace: &str, now_ms: u64) -> Result<(String, Vec<u8>), String> {
    let meta = Meta { workspace_name: store::folder_name(workspace), modified_ms: now_ms, ..Meta::default() };
    let bytes = pack(&store::write(data, &meta))?;
    Ok((store::board_name(workspace), bytes))
}

/// The board a file's bytes hold.
///
/// # Errors
/// The bytes are no board archive of this version ([`StoreError`]).
pub fn load(bytes: &[u8], limits: &Limits) -> Result<Loaded, StoreError> {
    if bytes.len() > MAX_ARCHIVE_BYTES {
        return Err(StoreError::TooLarge { bytes: bytes.len() });
    }
    // The reader unpacks every entry as soon as it parses the archive: the directory says first what that would be.
    if let Err(declared) = store::check_unpacked(bytes, limits) {
        return Err(StoreError::TooLarge { bytes: usize::try_from(declared).unwrap_or(usize::MAX) });
    }
    let zip = Zip::from_bytes(bytes.to_vec());
    if !zip.is_valid() || !zip.contains(store::MANIFEST) {
        return Err(StoreError::NotABoard("it is no archive with a board in it".to_string()));
    }
    let manifest = zip.get_file(store::MANIFEST);
    store::read(
        manifest.as_ref(),
        &|name| zip.contains(name).then(|| zip.get_file(name).as_ref().to_vec()),
        &AzulCodec,
        limits,
    )
}

/// What a load on its Thread is given, and answers with.
struct LoadInit {
    job: Option<(Vec<u8>, Limits)>,
    on_done: WriteBackCallbackType,
}

/// The answer of a load: the board, or why not.
pub struct LoadAnswer {
    pub result: Result<Loaded, StoreError>,
}

extern "C" fn load_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((bytes, limits), on_done)) = init
        .downcast_mut::<LoadInit>()
        .and_then(|mut i| Some((i.job.take()?, i.on_done)))
    else {
        return;
    };
    let result = load(&bytes, &limits);
    // In an Option, so the UI thread takes it out whole ([`take_loaded`]).
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(LoadAnswer { result })),
    )));
}

/// Decodes the board in `bytes` on an azul `Thread`; `on_done(app, message, info)` gets it on the UI
/// thread ([`take_loaded`]).
pub fn spawn_load(info: &mut CallbackInfo, app: &RefAny, bytes: Vec<u8>, limits: Limits, on_done: WriteBackCallbackType) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(LoadInit { job: Some((bytes, limits)), on_done }), app.clone(), load_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_loaded(msg: &mut RefAny) -> Option<Result<Loaded, StoreError>> {
    let mut guard = msg.downcast_mut::<Option<LoadAnswer>>()?;
    guard.take().map(|l| l.result)
}

pub use crate::whiteboard_export::write_atomic;

/// What a save on its Thread is given.
struct SaveInit {
    job: Option<(Vec<Entry>, PathBuf)>,
    on_done: WriteBackCallbackType,
}

/// The answer of a save: the file is written, or why not.
pub struct SaveAnswer {
    pub result: Result<(), String>,
}

extern "C" fn save_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((entries, path), on_done)) = init
        .downcast_mut::<SaveInit>()
        .and_then(|mut i| Some((i.job.take()?, i.on_done)))
    else {
        return;
    };
    let result = pack(&entries).and_then(|bytes| write_atomic(&path, &bytes));
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(SaveAnswer { result })),
    )));
}

/// Makes the zip of `entries` and writes it to `path` on an azul `Thread`; `on_done(app, message, info)` hears of it
/// on the UI thread ([`take_saved`]).
pub fn spawn_save(info: &mut CallbackInfo, app: &RefAny, entries: Vec<Entry>, path: PathBuf, on_done: WriteBackCallbackType) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(SaveInit { job: Some((entries, path)), on_done }), app.clone(), save_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_saved(msg: &mut RefAny) -> Option<Result<(), String>> {
    let mut guard = msg.downcast_mut::<Option<SaveAnswer>>()?;
    guard.take().map(|a| a.result)
}

// ---- a board as a file to share, on a Thread

/// What an export on its Thread is given.
struct ExportInit {
    job: Option<(ExportRequest, PathBuf)>,
    on_done: WriteBackCallbackType,
}

/// The answer of an export: the file is written, or why not.
pub struct ExportAnswer {
    pub result: Result<Exported, String>,
}

extern "C" fn export_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((request, path), on_done)) = init
        .downcast_mut::<ExportInit>()
        .and_then(|mut i| Some((i.job.take()?, i.on_done)))
    else {
        return;
    };
    let result = export::export(request, &path, export::GIT_TIMEOUT);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(ExportAnswer { result })),
    )));
}

/// Writes the board of `request` to `path` on an azul `Thread` (git is asked once there, for the remote and whether the tree
/// has changes); `on_done(app, message, info)` hears of it on the UI thread ([`take_exported`]).
pub fn spawn_export(info: &mut CallbackInfo, app: &RefAny, request: ExportRequest, path: PathBuf, on_done: WriteBackCallbackType) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(ExportInit { job: Some((request, path)), on_done }), app.clone(), export_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_exported(msg: &mut RefAny) -> Option<Result<Exported, String>> {
    let mut guard = msg.downcast_mut::<Option<ExportAnswer>>()?;
    guard.take().map(|a| a.result)
}

/// The board in the file at `path`: a document (`azboard/1`, with the pictures it names beside it) or the zip a board is
/// kept in (what is in a data folder, or was sent).
///
/// # Errors
/// The file cannot be read, is no board of this version, is of a newer one, or is larger than a board file is.
pub fn import_file(path: &Path, limits: &Limits) -> Result<Loaded, StoreError> {
    use std::io::Read;
    let mut head = [0_u8; 4];
    let is_zip = std::fs::File::open(path).and_then(|mut f| f.read_exact(&mut head)).is_ok() && &head == b"PK\x03\x04";
    if !is_zip {
        return export::import(path, &AzulCodec, limits);
    }
    let size = std::fs::metadata(path).map_err(|e| StoreError::NotABoard(format!("it cannot be read: {e}")))?.len();
    if size > MAX_ARCHIVE_BYTES as u64 {
        return Err(StoreError::TooLarge { bytes: usize::try_from(size).unwrap_or(usize::MAX) });
    }
    let bytes = std::fs::read(path).map_err(|e| StoreError::NotABoard(format!("it cannot be read: {e}")))?;
    load(&bytes, limits)
}

/// What an import on its Thread is given.
struct ImportInit {
    job: Option<(PathBuf, Limits)>,
    on_done: WriteBackCallbackType,
}

/// The answer of an import: the board, or why not; `file` is the file's name, for what is told.
pub struct ImportAnswer {
    pub result: Result<Loaded, StoreError>,
    pub file: String,
}

extern "C" fn import_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((path, limits), on_done)) = init
        .downcast_mut::<ImportInit>()
        .and_then(|mut i| Some((i.job.take()?, i.on_done)))
    else {
        return;
    };
    let file = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let result = import_file(&path, &limits);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_done,
        RefAny::new(Some(ImportAnswer { result, file })),
    )));
}

/// Reads the board in the file at `path` and decodes its pictures on an azul `Thread`; `on_done(app, message, info)` gets
/// it on the UI thread ([`take_imported`]).
pub fn spawn_import(info: &mut CallbackInfo, app: &RefAny, path: PathBuf, limits: Limits, on_done: WriteBackCallbackType) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(ImportInit { job: Some((path, limits)), on_done }), app.clone(), import_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_imported(msg: &mut RefAny) -> Option<ImportAnswer> {
    let mut guard = msg.downcast_mut::<Option<ImportAnswer>>()?;
    guard.take()
}

#[cfg(test)]
#[path = "whiteboard_archive_test.rs"]
mod whiteboard_archive_test;
