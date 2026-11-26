//! A folder of the tree is listed on a thread of its own and comes back as a message: the window never waits for a disk.
//!
//! The root is listed once, before the window opens (one `read_dir`); every other folder when the reviewer opens it. A
//! listing that comes back after another folder was chosen belongs to a workspace that is gone and is ignored
//! (`AppState::workspace`).

use std::sync::Arc;

use azul::prelude::*;

use crate::{
    symbols_source::{DirEntry, FileSource, ReadError},
    AppState,
};

/// What the worker is asked: list this folder of this workspace.
pub struct ListJob {
    pub source: Arc<dyn FileSource>,
    /// The workspace-relative path of the folder (`""`: the root).
    pub dir: String,
    /// Which workspace asked.
    pub workspace: u64,
}

/// What the worker answers.
struct Answer {
    workspace: u64,
    dir: String,
    result: Result<Vec<DirEntry>, ReadError>,
}

/// The worker thread: lists the folder and says so.
extern "C" fn worker(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some((source, dir, workspace)) = init
        .downcast_ref::<ListJob>()
        .map(|j| (j.source.clone(), j.dir.clone(), j.workspace))
    else {
        return;
    };
    let result = source.list(&dir);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        on_listed,
        RefAny::new(Answer { workspace, dir, result }),
    )));
}

/// A listing came back, on the window's thread: the tree takes it and the rows are built again.
extern "C" fn on_listed(mut app: RefAny, mut msg: RefAny, mut info: CallbackInfo) -> Update {
    let Some(answer) = msg
        .downcast_ref::<Answer>()
        .map(|a| (a.workspace, a.dir.clone(), a.result.clone()))
    else {
        return Update::DoNothing;
    };
    let changed = app
        .downcast_mut::<AppState>()
        .is_some_and(|mut s| s.take_listing(answer.0, &answer.1, answer.2));
    if changed {
        info.trigger_all_virtual_view_rerender();
        return Update::RefreshDom;
    }
    Update::DoNothing
}

/// Starts the thread that lists `job.dir`.
pub fn spawn(info: &mut CallbackInfo, app: &RefAny, job: ListJob) {
    info.add_thread(ThreadId::unique(), Thread::create(RefAny::new(job), app.clone(), worker));
}
