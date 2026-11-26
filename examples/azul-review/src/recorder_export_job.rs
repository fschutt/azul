//! One export of a recording to a video, run on a thread of its own so that the window stays alive: the recorder's
//! cut list for the recording as it was (`identity_edl`), turned into ffmpeg's commands and run. The window starts
//! it ([`ExportJob::start`]), looks at it from its timer ([`ExportJob::state`]: how far it is, what it made, why
//! it failed), can cancel it ([`ExportJob::cancel`]), and a job that is let go is cancelled and waited for, so
//! nothing keeps running behind the app.
//!
//! What does the work is a [`Renderer`]: [`FfmpegRenderer`] is the real one (the exporter of `recorder_export`), a
//! test brings its own. The video is `export/video.mp4` in the recording's folder, so that it goes with the
//! recording (and goes when the recording is deleted); the pieces ffmpeg makes on the way are in
//! `export/.work` and are removed at the end, whichever way it ends.

use std::{
    ffi::OsString,
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex,
    },
    thread::JoinHandle,
};

use crate::{
    recorder_edl::{file_changes, identity_edl},
    recorder_export::{build_plan, probe, run, ExportError},
    recorder_manifest::{Manifest, EVENTS_FILE, MANIFEST_FILE},
};

/// The folder in a recording's folder where its video is made.
pub const EXPORT_DIR: &str = "export";
/// The video's name in it.
pub const VIDEO_FILE: &str = "video.mp4";
/// Where the pieces of an export are made, in [`EXPORT_DIR`]; removed when it ends.
const WORK_DIR: &str = ".work";

/// The video of the recording in `session`, where an export makes it.
#[must_use]
pub fn video_path(session: &Path) -> PathBuf {
    session.join(EXPORT_DIR).join(VIDEO_FILE)
}

/// What an export made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Made {
    pub file: PathBuf,
    pub bytes: u64,
    /// What did not work as asked, one sentence each, in English (a rounded camera that came out as a rectangle).
    pub notes: Vec<String>,
}

/// Why a renderer did not make a video.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Failure {
    /// It was told to stop and did.
    Cancelled,
    /// It could not, and says why (the words of the system or of ffmpeg).
    Failed(String),
}

/// What turns a recording into a video. `cancel` is looked at between its steps (the real one looks every 50 ms);
/// `progress` is given how far it is, 0 to 1.
pub trait Renderer: Send + 'static {
    /// # Errors
    /// [`Failure::Cancelled`] when `cancel` was set and it stopped; [`Failure::Failed`] when it could not make the video.
    fn render(&mut self, session: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<Made, Failure>;
}

/// How an export stands.
#[derive(Debug, Clone, PartialEq)]
pub enum JobState {
    Running { fraction: f64 },
    Done(Made),
    Failed(String),
    Cancelled,
}

/// An export that is running or has ended.
pub struct ExportJob {
    state: Arc<Mutex<JobState>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl ExportJob {
    /// Starts exporting the recording in `session` with `renderer`, on a thread of its own. The job is running
    /// (at 0) when this returns; a thread that cannot be made is a failed job, with words.
    #[must_use]
    pub fn start(session: PathBuf, mut renderer: Box<dyn Renderer>) -> Self {
        let state = Arc::new(Mutex::new(JobState::Running { fraction: 0.0 }));
        let cancel = Arc::new(AtomicBool::new(false));
        let (worker_state, worker_cancel) = (Arc::clone(&state), Arc::clone(&cancel));
        let spawned = std::thread::Builder::new().name(String::from("recorder-export")).spawn(move || {
            let report = |fraction: f64| {
                // Only forward, only within 0 to 1: a second command that starts from nothing does not take the bar
                // back, and a number that is none leaves it where it is (`f64::max` gives the other of the two).
                if let Ok(mut s) = worker_state.lock() {
                    if let JobState::Running { fraction: shown } = &mut *s {
                        *shown = shown.max(fraction.clamp(0.0, 1.0));
                    }
                }
            };
            let outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| renderer.render(&session, &worker_cancel, &mut { report })));
            let last = match outcome {
                Ok(Ok(made)) => JobState::Done(made),
                Ok(Err(Failure::Cancelled)) => JobState::Cancelled,
                Ok(Err(Failure::Failed(why))) => JobState::Failed(why),
                Err(_) => JobState::Failed(String::from("the export stopped unexpectedly")),
            };
            if let Ok(mut s) = worker_state.lock() {
                *s = last;
            }
        });
        match spawned {
            Ok(thread) => Self { state, cancel, thread: Some(thread) },
            Err(e) => {
                if let Ok(mut s) = state.lock() {
                    *s = JobState::Failed(format!("the export could not start: {e}"));
                }
                Self { state, cancel, thread: None }
            }
        }
    }

    /// How the export stands now.
    #[must_use]
    pub fn state(&self) -> JobState {
        self.state.lock().map_or_else(|_| JobState::Failed(String::from("the export stopped unexpectedly")), |s| s.clone())
    }

    /// The export is still going.
    #[must_use]
    pub fn is_running(&self) -> bool {
        matches!(self.state(), JobState::Running { .. })
    }

    /// Asks it to stop. A video that was made before the cancel came stays.
    pub fn cancel(&self) {
        self.cancel.store(true, Ordering::SeqCst);
    }
}

impl Drop for ExportJob {
    fn drop(&mut self) {
        self.cancel();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

// ---- where ffmpeg is ----

/// The folders to look for a program in: the search path's, then where a Mac's package managers put theirs (a
/// program started from the Finder has a short search path that has none of them).
#[must_use]
pub fn program_places(search_path: Option<OsString>) -> Vec<PathBuf> {
    let mut places: Vec<PathBuf> = search_path.map(|p| std::env::split_paths(&p).collect()).unwrap_or_default();
    places.extend(["/opt/homebrew/bin", "/usr/local/bin", "/usr/bin"].iter().map(PathBuf::from));
    places
}

/// `name` in the first of `places` that has it as a file that can be run.
#[must_use]
pub fn find_program(name: &str, places: &[PathBuf]) -> Option<PathBuf> {
    places.iter().map(|dir| dir.join(name)).find(|p| can_run(p))
}

/// The program to run: `wanted` (the one the settings name) when it can be run, else the first of `places` that has
/// `name`, with a note that names both (a wrong path in the settings is never silently another program); `None` when
/// there is none.
#[must_use]
pub fn choose_program(name: &str, wanted: Option<&Path>, places: &[PathBuf]) -> Option<(PathBuf, Option<String>)> {
    if let Some(wanted) = wanted.filter(|w| can_run(w)) {
        return Some((wanted.to_path_buf(), None));
    }
    let found = find_program(name, places)?;
    let note = wanted.map(|w| format!("{} cannot be run, as the settings say: {} is used instead", w.display(), found.display()));
    Some((found, note))
}

#[cfg(unix)]
fn can_run(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

#[cfg(not(unix))]
fn can_run(path: &Path) -> bool {
    path.is_file()
}

// ---- the real renderer ----

/// What an export says when there is no ffmpeg.
pub const NO_FFMPEG: &str = "ffmpeg was not found: the export needs it (on a Mac: brew install ffmpeg)";

/// The exporter of `recorder_export` on the recording as it was: one segment for each run, the camera in its corner,
/// the voice, a chapter for each file the review opened.
#[derive(Debug, Clone, Default)]
pub struct FfmpegRenderer {
    /// Exactly this program, or `None` to choose one (below).
    pub ffmpeg: Option<PathBuf>,
    /// The program the settings name: used when it can be run, else one is looked for ([`program_places`]) and the
    /// export says that it was not the one asked for.
    pub preferred: Option<PathBuf>,
    /// Where to make the video, or `None` for [`video_path`].
    pub out: Option<PathBuf>,
}

impl Renderer for FfmpegRenderer {
    fn render(&mut self, session: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<Made, Failure> {
        let failed = |why: String| Failure::Failed(why);
        let (ffmpeg, which) = match &self.ffmpeg {
            Some(exactly) => (exactly.clone(), None),
            None => choose_program("ffmpeg", self.preferred.as_deref(), &program_places(std::env::var_os("PATH"))).ok_or_else(|| failed(String::from(NO_FFMPEG)))?,
        };
        let manifest_text = fs::read_to_string(session.join(MANIFEST_FILE)).map_err(|e| failed(format!("the recording's notes cannot be read: {e}")))?;
        let manifest = Manifest::parse(&manifest_text).map_err(|e| failed(format!("the recording's notes cannot be read: {e}")))?;
        // The events are what the chapters are made of; a recording without them (or with a torn last line) has none.
        let files = file_changes(&fs::read_to_string(session.join(EVENTS_FILE)).unwrap_or_default());
        let edl = identity_edl(&manifest, &files);
        let problems = edl.problems(&manifest);
        if !problems.is_empty() {
            return Err(failed(problems.join("; ")));
        }
        let found = probe(&ffmpeg).map_err(failed)?;
        let out = self.out.clone().unwrap_or_else(|| video_path(session));
        let work = session.join(EXPORT_DIR).join(WORK_DIR);
        let plan = build_plan(&manifest, &edl, session, &work, &out, found.caps).map_err(failed)?;
        let result = run(&plan, &ffmpeg, cancel, &mut |p| progress(p.fraction));
        // The pieces are gone after a run whichever way it ended; the folder they were made in goes too.
        let _ = fs::remove_dir(&work);
        match result {
            Ok(report) => {
                let mut notes: Vec<String> = which.into_iter().collect();
                notes.extend(found.notes);
                notes.extend(plan.notes);
                Ok(Made { file: report.output, bytes: report.bytes, notes })
            }
            Err(ExportError::Cancelled) => Err(Failure::Cancelled),
            Err(other) => Err(failed(other.to_string())),
        }
    }
}

#[cfg(test)]
#[path = "recorder_export_job_test.rs"]
mod recorder_export_job_test;
