//! The one tokio runtime of the async back-ends: OpenDAL's services and the database drivers
//! (sqlx) are async, a [`crate::Drive`] is blocking. A drive's call runs its future to the end
//! on the calling thread (an azul `Thread`, never a UI callback) with [`block_on`]; the
//! runtime's two workers drive the sockets and timers meanwhile and run what the drivers spawn
//! (sqlx's connection tasks). One runtime for the process: a connection pool belongs to the
//! runtime that made it.

use std::{future::Future, sync::OnceLock};

use crate::DriveError;

/// Worker threads: the sockets of a few drives, never CPU work.
const WORKERS: usize = 2;

/// The runtime, made on first use; why it could not be made, if it could not.
pub fn shared() -> Result<&'static tokio::runtime::Runtime, DriveError> {
    static RUNTIME: OnceLock<Result<tokio::runtime::Runtime, String>> = OnceLock::new();
    RUNTIME
        .get_or_init(|| {
            tokio::runtime::Builder::new_multi_thread()
                .worker_threads(WORKERS)
                .thread_name("azul-storage")
                .enable_all()
                .build()
                .map_err(|e| e.to_string())
        })
        .as_ref()
        .map_err(|e| DriveError::Io(format!("the storage runtime could not start: {e}")))
}

/// Runs `future` to its end on this thread. Call it from a plain thread (an azul `Thread`, a
/// test), never from inside the runtime: tokio refuses to block one of its own workers.
pub fn block_on<F: Future>(future: F) -> Result<F::Output, DriveError> {
    Ok(shared()?.block_on(future))
}
