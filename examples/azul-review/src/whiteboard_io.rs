//! The reads and writes of the whiteboard's files that are not the board's own save, each on a Thread of its own so that the
//! window never waits for a disk: what the start reads (the list of boards and whether the one-board version's file is there),
//! the board that is open (read, and its pictures decoded), and the small files the owner asks to have written (the list of boards,
//! the copy that keeps a damaged file).
//!
//! The same shape as `whiteboard_archive`'s `spawn_*` (a Thread given a job, a write-back with the answer in an `Option` that
//! the UI thread takes out whole), so that the app's callbacks handle all of them alike.

use std::path::{Path, PathBuf};

use azul::{
    callbacks::{CallbackInfo, RefAny, WriteBackCallbackType},
    task::{Thread, ThreadId, ThreadReceiveMsg, ThreadReceiver, ThreadSender, ThreadWriteBackMsg},
};

use crate::{
    whiteboard::Limits,
    whiteboard_app::Effect,
    whiteboard_archive::{self, write_atomic},
    whiteboard_store::{Loaded, StoreError},
};

/// The most bytes of the list of boards that are read: a list holds a few hundred bytes a board.
pub const MAX_INDEX_BYTES: u64 = 4 * 1024 * 1024;

// ---- the start

/// What the start found in the data folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Started {
    /// The bytes of the list of boards; `None` when there is no such file (or it could not be read).
    pub index: Option<Vec<u8>>,
    /// Whether the one-board version's file of this workspace is there.
    pub legacy_file: bool,
}

/// Reads the file `index` (its first [`MAX_INDEX_BYTES`] and a byte more, so that one longer than a list can be is seen to be:
/// the owner keeps what it could not use aside) and looks whether `legacy` is there.
#[must_use]
pub fn look(index: &Path, legacy: &Path) -> Started {
    use std::io::Read as _;
    let index = std::fs::File::open(index).ok().and_then(|f| {
        let mut bytes = Vec::new();
        f.take(MAX_INDEX_BYTES + 1).read_to_end(&mut bytes).ok().map(|_| bytes)
    });
    Started { index, legacy_file: legacy.is_file() }
}

struct StartInit {
    job: Option<(PathBuf, PathBuf)>,
    on_done: WriteBackCallbackType,
}

extern "C" fn start_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((index, legacy), on_done)) = init.downcast_mut::<StartInit>().and_then(|mut i| Some((i.job.take()?, i.on_done)))
    else {
        return;
    };
    let answer = look(&index, &legacy);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(on_done, RefAny::new(Some(answer)))));
}

/// Looks at the data folder on a Thread: `on_done(app, message, info)` gets the answer ([`take_started`]).
pub fn spawn_start(info: &mut CallbackInfo, app: &RefAny, index: PathBuf, legacy: PathBuf, on_done: WriteBackCallbackType) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(StartInit { job: Some((index, legacy)), on_done }), app.clone(), start_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_started(msg: &mut RefAny) -> Option<Started> {
    msg.downcast_mut::<Option<Started>>()?.take()
}

// ---- a board

/// What reading a board's file came to.
pub struct Read {
    /// The bytes, kept for a copy aside when they are no board (`None`: there is no file).
    pub raw: Option<Vec<u8>>,
    /// What they hold (`None`: there is no file).
    pub result: Option<Result<Loaded, StoreError>>,
}

/// Reads the board at `path` and decodes it; no file is `Read { raw: None, result: None }`.
#[must_use]
pub fn read_board(path: &Path, limits: &Limits) -> Read {
    match std::fs::read(path) {
        Ok(bytes) => {
            let result = whiteboard_archive::load(&bytes, limits);
            Read { raw: Some(bytes), result: Some(result) }
        }
        Err(_) => Read { raw: None, result: None },
    }
}

struct ReadInit {
    job: Option<(PathBuf, Limits)>,
    on_done: WriteBackCallbackType,
}

extern "C" fn read_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((path, limits), on_done)) = init.downcast_mut::<ReadInit>().and_then(|mut i| Some((i.job.take()?, i.on_done)))
    else {
        return;
    };
    let answer = read_board(&path, &limits);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(on_done, RefAny::new(Some(answer)))));
}

/// Reads and decodes the board at `path` on a Thread: `on_done(app, message, info)` gets it ([`take_read`]).
pub fn spawn_read(info: &mut CallbackInfo, app: &RefAny, path: PathBuf, limits: Limits, on_done: WriteBackCallbackType) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(ReadInit { job: Some((path, limits)), on_done }), app.clone(), read_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_read(msg: &mut RefAny) -> Option<Read> {
    msg.downcast_mut::<Option<Read>>()?.take()
}

// ---- small files

/// What writing the files came to: the name and what went wrong, for each that did.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Written {
    pub failed: Vec<(String, String)>,
    pub written: usize,
}

/// Writes each effect to `files/<name>`, atomically; one that cannot be written does not stop the others.
#[must_use]
pub fn write_effects(files: &Path, effects: &[Effect]) -> Written {
    let mut out = Written::default();
    for e in effects {
        // a name that leaves the folder is never written
        if e.name.split('/').any(|part| part == ".." || part.is_empty()) || e.name.starts_with('/') || e.name.contains('\\') {
            out.failed.push((e.name.clone(), "the name leaves the folder".to_string()));
            continue;
        }
        match write_atomic(&files.join(&e.name), &e.bytes) {
            Ok(()) => out.written += 1,
            Err(why) => out.failed.push((e.name.clone(), why)),
        }
    }
    out
}

struct WriteInit {
    job: Option<(PathBuf, Vec<Effect>)>,
    on_done: WriteBackCallbackType,
}

extern "C" fn write_worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(((files, effects), on_done)) = init.downcast_mut::<WriteInit>().and_then(|mut i| Some((i.job.take()?, i.on_done)))
    else {
        return;
    };
    let answer = write_effects(&files, &effects);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(on_done, RefAny::new(Some(answer)))));
}

/// Writes the effects on a Thread: `on_done(app, message, info)` hears how it went ([`take_written`]). Nothing to write
/// starts nothing.
pub fn spawn_write(info: &mut CallbackInfo, app: &RefAny, files: PathBuf, effects: Vec<Effect>, on_done: WriteBackCallbackType) {
    if effects.is_empty() {
        return;
    }
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(WriteInit { job: Some((files, effects)), on_done }), app.clone(), write_worker),
    );
}

/// The answer out of a write-back's message (`None` if it is not one).
#[must_use]
pub fn take_written(msg: &mut RefAny) -> Option<Written> {
    msg.downcast_mut::<Option<Written>>()?.take()
}

#[cfg(test)]
#[path = "whiteboard_io_test.rs"]
mod whiteboard_io_test;
