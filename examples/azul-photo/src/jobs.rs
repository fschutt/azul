//! Work off the UI thread: reading and decoding image files, saving and
//! loading documents through the drive, listing them, encoding exports.
//!
//! Each job runs on an azul `Thread`; its outcome comes back through the
//! thread's write-back (`on_job_done` in lib.rs). No callback waits on a
//! disk, a network drive or an encoder.

use std::{path::PathBuf, sync::Arc};

use azul::prelude::*;
use azul_storage::Drive;

use crate::{
    codec,
    raster::Document,
    storage::{self, DocEntry, Saved},
};

/// The export formats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Png,
    Jpeg,
}

impl ExportFormat {
    #[must_use]
    pub const fn extension(self) -> &'static str {
        match self {
            Self::Png => "png",
            Self::Jpeg => "jpg",
        }
    }
}

/// One piece of work.
pub enum Job {
    /// Read and decode an image file (Open, Place).
    OpenFile { path: PathBuf, as_layer: bool },
    /// Write the document (tiles, then doc.json).
    Save {
        drive: Arc<dyn Drive>,
        uuid: String,
        name: String,
        doc: Document,
    },
    /// Read a saved document.
    Load { drive: Arc<dyn Drive>, uuid: String },
    /// The saved documents, for the start screen.
    List { drive: Arc<dyn Drive> },
    /// Encode the flattened image and write it.
    Export {
        path: PathBuf,
        format: ExportFormat,
        quality: u8,
        width: u32,
        height: u32,
        rgba: Vec<u8>,
    },
}

/// What a job produced.
pub enum Outcome {
    Opened {
        name: String,
        as_layer: bool,
        result: Result<(u32, u32, Vec<u8>), String>,
    },
    Saved {
        uuid: String,
        result: Result<Saved, String>,
    },
    Loaded {
        uuid: String,
        result: Result<(String, Document), String>,
    },
    Listed(Result<Vec<DocEntry>, String>),
    Exported {
        path: PathBuf,
        result: Result<u64, String>,
    },
}

/// Run one job (on the worker thread).
#[must_use]
pub fn run(job: Job) -> Outcome {
    match job {
        Job::OpenFile { path, as_layer } => {
            let name = path
                .file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string());
            let result = std::fs::read(&path)
                .map_err(|e| format!("{}: {e}", path.display()))
                .and_then(|bytes| codec::decode(&bytes));
            Outcome::Opened {
                name,
                as_layer,
                result,
            }
        }
        Job::Save {
            drive,
            uuid,
            name,
            doc,
        } => Outcome::Saved {
            result: storage::save(drive.as_ref(), &uuid, &name, &doc, &codec::encode_png),
            uuid,
        },
        Job::Load { drive, uuid } => Outcome::Loaded {
            result: storage::load(drive.as_ref(), &uuid, &codec::decode),
            uuid,
        },
        Job::List { drive } => Outcome::Listed(storage::list(drive.as_ref())),
        Job::Export {
            path,
            format,
            quality,
            width,
            height,
            rgba,
        } => {
            let bytes = match format {
                ExportFormat::Png => codec::encode_png(width, height, &rgba),
                ExportFormat::Jpeg => codec::encode_jpeg(width, height, &rgba, quality),
            };
            let result = bytes.and_then(|b| {
                if let Some(dir) = path.parent() {
                    let _ = std::fs::create_dir_all(dir);
                }
                std::fs::write(&path, &b)
                    .map(|()| b.len() as u64)
                    .map_err(|e| format!("{}: {e}", path.display()))
            });
            Outcome::Exported { path, result }
        }
    }
}

/// The thread's start data.
pub struct JobInit {
    pub job: Option<Job>,
}

/// The write-back's payload.
pub struct Done {
    pub outcome: Option<Outcome>,
}

/// The worker: run the job, send the outcome back.
pub extern "C" fn job_thread(mut init: RefAny, mut sender: ThreadSender, _receiver: ThreadReceiver) {
    let Some(job) = init
        .downcast_mut::<JobInit>()
        .and_then(|mut init| init.job.take())
    else {
        return;
    };
    let outcome = run(job);
    let _sent = sender.send(ThreadReceiveMsg::WriteBack(ThreadWriteBackMsg::create(
        crate::on_job_done,
        RefAny::new(Done {
            outcome: Some(outcome),
        }),
    )));
}

/// Start `job` on a new thread; its outcome reaches `app` through
/// `on_job_done`.
pub fn spawn(info: &mut CallbackInfo, app: &RefAny, job: Job) {
    info.add_thread(
        ThreadId::unique(),
        Thread::create(RefAny::new(JobInit { job: Some(job) }), app.clone(), job_thread),
    );
}
