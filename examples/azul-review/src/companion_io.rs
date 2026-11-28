//! The companion's file, read and written on a Thread of its own so that the window never waits for a disk: what is kept at the start
//! (a few hundred bytes: the switch, the key, the paired phones) and what a change of it writes.
//!
//! The file holds a secret (this computer's key) and the list of who may command it, so it is written for the user only: the folders
//! this makes are `0700` and the file `0600` (on a system that has such modes), through a file of its own beside it that is renamed
//! over it, so that a crash leaves the old file or the new one, never half of one. The same shape as `whiteboard_io`'s Threads (a
//! Thread given a job, a write-back with the answer in an `Option` that the UI thread takes out whole).

use std::{
    io::{Read as _, Write as _},
    path::{Path, PathBuf},
};

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};

use crate::companion_store::MAX_FILE_BYTES;

/// What reading the file came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    /// The text; `None` when there is no file, or when it could not be used (then `problem` says why).
    pub text: Option<String>,
    /// Why a file that is there was not read.
    pub problem: Option<String>,
}

/// Reads the file at `path`: its first [`MAX_FILE_BYTES`] and a byte more, so that a file longer than any the companion writes is
/// seen to be (and is not used: it is no file of ours).
#[must_use]
pub fn look(path: &Path) -> Loaded {
    let file = match std::fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Loaded { text: None, problem: None },
        Err(e) => return Loaded { text: None, problem: Some(format!("the companion's file cannot be opened: {e}")) },
    };
    let mut bytes = Vec::new();
    if let Err(e) = file.take(MAX_FILE_BYTES + 1).read_to_end(&mut bytes) {
        return Loaded { text: None, problem: Some(format!("the companion's file cannot be read: {e}")) };
    }
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Loaded { text: None, problem: Some(format!("the companion's file is longer than {MAX_FILE_BYTES} bytes: it is no file of the companion's")) };
    }
    match String::from_utf8(bytes) {
        Ok(text) => Loaded { text: Some(text), problem: None },
        Err(_) => Loaded { text: None, problem: Some("the companion's file is not text".to_string()) },
    }
}

/// Writes `bytes` to `path` for the user only: the folder is made if need be, the bytes go to a file of their own beside it and that
/// is renamed over `path`.
///
/// # Errors
/// What the file system said, or that the bytes are longer than the companion's file may be.
pub fn write_private(path: &Path, bytes: &[u8]) -> Result<(), String> {
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(format!("the companion's file would be {} bytes: more than the {MAX_FILE_BYTES} it may be", bytes.len()));
    }
    if let Some(dir) = path.parent() {
        make_private_dir(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("companion");
    let beside = path.with_file_name(format!(".{name}.tmp"));
    let written = open_private(&beside).and_then(|mut file| {
        file.write_all(bytes)?;
        file.sync_all()
    });
    if let Err(e) = written {
        let _gone = std::fs::remove_file(&beside);
        return Err(format!("{}: {e}", beside.display()));
    }
    std::fs::rename(&beside, path).map_err(|e| {
        let _gone = std::fs::remove_file(&beside);
        format!("{}: {e}", path.display())
    })
}

#[cfg(unix)]
fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    use std::os::unix::fs::DirBuilderExt as _;
    std::fs::DirBuilder::new().recursive(true).mode(0o700).create(dir)
}

#[cfg(not(unix))]
fn make_private_dir(dir: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dir)
}

#[cfg(unix)]
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    use std::os::unix::fs::OpenOptionsExt as _;
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).open(path)
}

#[cfg(not(unix))]
fn open_private(path: &Path) -> std::io::Result<std::fs::File> {
    std::fs::OpenOptions::new().write(true).create(true).truncate(true).open(path)
}

// ---- the Threads

struct LoadInit {
    path: Option<PathBuf>,
    on_done: WriteBackCallbackType,
}

extern "C" fn load_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((path, on_done)) = init.downcast_mut::<LoadInit>().and_then(|mut i| Some((i.path.take()?, i.on_done))) else {
        return;
    };
    let answer = look(&path);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(on_done, RefAny::new(Some(answer)))));
}

/// Reads the file at `path` on a Thread: `on_done(app, message, info)` gets the answer ([`take_loaded`]).
pub fn spawn_load(info: &mut CallbackInfo, app: &RefAny, path: PathBuf, on_done: WriteBackCallbackType) {
    info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(LoadInit { path: Some(path), on_done }), app.clone(), load_worker));
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_loaded(msg: &mut RefAny) -> Option<Loaded> {
    msg.downcast_mut::<Option<Loaded>>()?.take()
}

/// How a write went.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Saved {
    /// What went wrong (`None`: the file is written).
    pub error: Option<String>,
}

struct SaveInit {
    job: Option<(PathBuf, Vec<u8>)>,
    on_done: WriteBackCallbackType,
}

extern "C" fn save_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((path, bytes), on_done)) = init.downcast_mut::<SaveInit>().and_then(|mut i| Some((i.job.take()?, i.on_done))) else {
        return;
    };
    let answer = Saved { error: write_private(&path, &bytes).err() };
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(on_done, RefAny::new(Some(answer)))));
}

/// Writes `bytes` to `path` on a Thread: `on_done(app, message, info)` hears how it went ([`take_saved`]).
pub fn spawn_save(info: &mut CallbackInfo, app: &RefAny, path: PathBuf, bytes: Vec<u8>, on_done: WriteBackCallbackType) {
    info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(SaveInit { job: Some((path, bytes)), on_done }), app.clone(), save_worker));
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_saved(msg: &mut RefAny) -> Option<Saved> {
    msg.downcast_mut::<Option<Saved>>()?.take()
}

#[cfg(test)]
#[path = "companion_io_test.rs"]
mod companion_io_test;
