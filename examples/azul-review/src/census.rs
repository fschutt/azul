//! Counting the workspace: every folder listed from the root down and the lines of every file counted, on a thread that is
//! not the window's, so that "N of M lines reviewed" has its M.
//!
//! The walk follows the tree's rules (`tree::shown`: one function) and sends the window one message per folder (the listing
//! as the source gave it and the lines of the files in it), which the tree takes as it takes any listing. It reads each file
//! once (the symbol index reads them too: two reads of a tree that the system keeps in memory, one after the other), and
//! stops when the workspace changes or the app is told to. A file that a window would not show (no text, too big) has no
//! lines.

use std::{
    collections::VecDeque,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
};

use azul::prelude::*;

use crate::{
    code,
    symbols_source::{DirEntry, EntryKind, FileSource},
    tree::{self, Rules},
    AppState,
};

/// What the walk sends for a folder.
#[derive(Debug, Clone)]
pub struct Batch {
    /// The workspace-relative path of the folder (`""`: the root).
    pub dir: String,
    /// What the source listed in it, whole (the tree applies the rules).
    pub entries: Vec<DirEntry>,
    /// The lines of the files of it that the rules show: (name, lines).
    pub lines: Vec<(String, u32)>,
}

/// What a walk counted.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Counted {
    pub dirs: usize,
    pub files: usize,
    pub lines: u64,
}

/// How many lines the bytes of a file have, as the page counts them: a line end ends a line, and what is left after the
/// last one is a line.
#[must_use]
pub fn count_lines(bytes: &[u8]) -> u32 {
    let ends = bytes.iter().filter(|&&b| b == b'\n').count();
    let last_open = bytes.last().is_some_and(|&b| b != b'\n');
    u32::try_from(ends + usize::from(last_open)).unwrap_or(u32::MAX)
}

/// The lines of the file `path` as a page would show them; none for one it would not show.
fn file_lines(source: &dyn FileSource, path: &str) -> u32 {
    match source.read(path, code::MAX_OPEN_BYTES) {
        Ok(bytes) if !bytes[..bytes.len().min(8192)].contains(&0) => count_lines(&bytes),
        _ => 0,
    }
}

/// Walks the workspace of `source` from the root, a folder after the folder before it at the same depth, handing each
/// folder to `on_batch`, until it has been all counted or `stop` is set.
pub fn walk(source: &dyn FileSource, stop: &AtomicBool, on_batch: &mut dyn FnMut(Batch)) -> Counted {
    let rules = Rules::default();
    let mut counted = Counted::default();
    let mut queue: VecDeque<(String, usize)> = VecDeque::from([(String::new(), 0)]);
    while let Some((dir, depth)) = queue.pop_front() {
        if stop.load(Ordering::Relaxed) {
            break;
        }
        let Ok(entries) = source.list(&dir) else {
            continue;
        };
        let (kept, _) = tree::shown(&entries, depth, &rules);
        let mut lines = Vec::new();
        for entry in kept {
            let path = if dir.is_empty() { entry.name.clone() } else { format!("{dir}/{}", entry.name) };
            match entry.kind {
                EntryKind::Dir => queue.push_back((path, depth + 1)),
                EntryKind::File => {
                    let n = file_lines(source, &path);
                    counted.files += 1;
                    counted.lines += u64::from(n);
                    lines.push((entry.name.clone(), n));
                }
                EntryKind::Other => {}
            }
        }
        counted.dirs += 1;
        on_batch(Batch { dir, entries, lines });
    }
    counted
}

/// What the worker thread is asked.
pub struct CensusJob {
    pub source: Arc<dyn FileSource>,
    pub workspace: u64,
    pub stop: Arc<AtomicBool>,
}

/// What the worker answers.
struct Message {
    workspace: u64,
    batch: Batch,
}

extern "C" fn worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((source, workspace, stop)) = init
        .downcast_ref::<CensusJob>()
        .map(|j| (j.source.clone(), j.workspace, j.stop.clone()))
    else {
        return;
    };
    walk(source.as_ref(), &stop, &mut |batch| {
        let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
            on_batch,
            RefAny::new(Message { workspace, batch }),
        )));
    });
}

/// A folder was counted, on the window's thread: the tree takes the listing and the lines; the window is built again only
/// where a row changed (the totals are built again by the next tick of the watcher).
extern "C" fn on_batch(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some((workspace, batch)) = msg.downcast_ref::<Message>().map(|m| (m.workspace, m.batch.clone())) else {
        return Update::DoNothing;
    };
    let rows_changed = app
        .downcast_mut::<AppState>()
        .is_some_and(|mut s| s.take_census(workspace, batch));
    if rows_changed {
        info.trigger_all_virtual_view_rerender();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// Starts counting the workspace the app has now (the walk of a workspace that was left is stopped).
pub fn start(info: &mut CallbackInfo, app: &RefAny) {
    let mut handle = app.clone();
    let job = handle.downcast_mut::<AppState>().map(|mut s| {
        if let Some(old) = s.census_stop.take() {
            old.store(true, Ordering::Relaxed);
        }
        let stop = Arc::new(AtomicBool::new(false));
        s.census_stop = Some(stop.clone());
        CensusJob { source: s.source.clone(), workspace: s.workspace, stop }
    });
    if let Some(job) = job {
        info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(job), app.clone(), worker));
    }
}

#[cfg(test)]
#[path = "census_test.rs"]
mod census_test;
