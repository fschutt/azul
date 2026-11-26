//! An export that runs on a thread of its own: how far it is, what it made, why it failed, that it can be
//! cancelled and that nothing is left running when the person goes. On a renderer the test drives step by step;
//! the real one (ffmpeg) is in the end-to-end tests of `recorder_export_test.rs`.

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc::{channel, Receiver, Sender},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};

use super::*;

/// What the test tells its renderer to do next.
enum Step {
    Progress(f64),
    Finish(Made),
    Fail(&'static str),
    Cancelled,
    Panic,
    /// Keeps working until the cancel comes, and then has made the video after all.
    FinishWhenCancelled(Made),
}

/// A renderer that does what it is told, one step at a time, and says what it was given.
struct Scripted {
    steps: Receiver<Step>,
    saw_cancel: Arc<AtomicBool>,
    given: Arc<Mutex<Option<PathBuf>>>,
}

impl Renderer for Scripted {
    fn render(&mut self, session: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<Made, Failure> {
        *self.given.lock().unwrap() = Some(session.to_path_buf());
        let mut finished = None;
        loop {
            // Waits for the test's next word, looking at the cancel flag between (as the real one does between two looks).
            match self.steps.recv_timeout(Duration::from_millis(20)) {
                Ok(Step::Progress(f)) => progress(f),
                Ok(Step::Finish(made)) => return Ok(made),
                Ok(Step::Fail(why)) => return Err(Failure::Failed(why.to_string())),
                Ok(Step::Cancelled) => return Err(Failure::Cancelled),
                Ok(Step::Panic) => panic!("the renderer broke"),
                Ok(Step::FinishWhenCancelled(made)) => finished = Some(made),
                Err(_) => {
                    if cancel.load(Ordering::Relaxed) {
                        self.saw_cancel.store(true, Ordering::SeqCst);
                        return finished.take().ok_or(Failure::Cancelled);
                    }
                }
            }
        }
    }
}

struct Rig {
    job: ExportJob,
    tell: Sender<Step>,
    saw_cancel: Arc<AtomicBool>,
    given: Arc<Mutex<Option<PathBuf>>>,
}

fn scripted() -> Rig {
    let (tell, steps) = channel();
    let (saw_cancel, given) = (Arc::new(AtomicBool::new(false)), Arc::new(Mutex::new(None)));
    let renderer = Scripted { steps, saw_cancel: Arc::clone(&saw_cancel), given: Arc::clone(&given) };
    Rig { job: ExportJob::start(PathBuf::from("/rec/2026-11-26_10-00-00"), Box::new(renderer)), tell, saw_cancel, given }
}

/// Waits (at most five seconds) until the job's state is `want`.
fn until(job: &ExportJob, want: &JobState) {
    let from = Instant::now();
    while job.state() != *want {
        assert!(from.elapsed() < Duration::from_secs(5), "the job stayed {:?}, wanted {want:?}", job.state());
        std::thread::sleep(Duration::from_millis(2));
    }
}

fn made() -> Made {
    Made { file: PathBuf::from("/rec/2026-11-26_10-00-00/export/video.mp4"), bytes: 1_234_567, notes: vec![String::from("this ffmpeg cannot round the camera: a rectangle")] }
}

/// A job is running from the moment it starts (a window that asks at once is told so), the renderer is given the
/// recording's folder, the fraction it reports is what the state says, and what it made is what the job says it made.
///
/// Mutation: a job that starts as nothing; a renderer given another folder; a fraction that is not passed on; a
/// finished job that stays running.
#[test]
fn a_job_runs_from_its_start_says_how_far_it_is_and_what_it_made() {
    let r = scripted();
    assert_eq!(r.job.state(), JobState::Running { fraction: 0.0 }, "running before the renderer has said a word");
    assert!(r.job.is_running());
    r.tell.send(Step::Progress(0.4)).unwrap();
    until(&r.job, &JobState::Running { fraction: 0.4 });
    assert_eq!(r.given.lock().unwrap().as_deref(), Some(Path::new("/rec/2026-11-26_10-00-00")));
    r.tell.send(Step::Finish(made())).unwrap();
    until(&r.job, &JobState::Done(made()));
    assert!(!r.job.is_running());
}

/// What the renderer reports is kept in range and only goes forward: a fraction under 0 is 0, over 1 is 1, one that
/// is no number is not taken, and a renderer that says less than it said before (a second command that starts from
/// nothing) does not take the bar back.
///
/// Mutation: no upper limit; no lower limit; a number that is not a number taken; a bar that goes back.
#[test]
fn the_fraction_is_kept_between_nothing_and_all_and_never_goes_back() {
    let r = scripted();
    let seen = |fraction: f64, r: &Rig| {
        r.tell.send(Step::Progress(fraction)).unwrap();
        // The renderer takes the word within its 20 ms look; the state is read after it did.
        std::thread::sleep(Duration::from_millis(80));
        match r.job.state() {
            JobState::Running { fraction } => fraction,
            other => panic!("{other:?}"),
        }
    };
    assert_eq!(seen(0.5, &r), 0.5);
    assert_eq!(seen(0.3, &r), 0.5, "not back");
    assert_eq!(seen(f64::NAN, &r), 0.5, "no number");
    assert_eq!(seen(-4.0, &r), 0.5);
    assert_eq!(seen(1.7, &r), 1.0, "at most all");
    assert_eq!(seen(f64::INFINITY, &r), 1.0);
    r.tell.send(Step::Cancelled).unwrap();
}

/// A renderer that fails says why, in the words it gave; one that panics is a failure with words too, not a crash
/// and not a job that is running for ever.
///
/// Mutation: a failure that loses its words; a panic that leaves the job running.
#[test]
fn a_failure_is_said_and_a_renderer_that_breaks_is_a_failure_not_a_crash() {
    let r = scripted();
    r.tell.send(Step::Fail("ffmpeg failed at run1 (exit 1): no such file")).unwrap();
    until(&r.job, &JobState::Failed(String::from("ffmpeg failed at run1 (exit 1): no such file")));
    assert!(!r.job.is_running());

    let r = scripted();
    r.tell.send(Step::Panic).unwrap();
    let from = Instant::now();
    while r.job.is_running() {
        assert!(from.elapsed() < Duration::from_secs(5), "a panic left the job running");
        std::thread::sleep(Duration::from_millis(2));
    }
    match r.job.state() {
        JobState::Failed(why) => assert!(why.contains("stopped unexpectedly"), "{why}"),
        other => panic!("{other:?}"),
    }
}

/// Cancelling reaches the renderer (it sees the flag), and the job says it was cancelled; a cancel that comes after
/// the video was made leaves the video.
///
/// Mutation: a cancel that is not passed on; a finished job turned into a cancelled one.
#[test]
fn a_cancel_reaches_the_renderer_and_a_cancel_after_the_end_changes_nothing() {
    let r = scripted();
    r.tell.send(Step::Progress(0.2)).unwrap();
    until(&r.job, &JobState::Running { fraction: 0.2 });
    r.job.cancel();
    until(&r.job, &JobState::Cancelled);
    assert!(r.saw_cancel.load(Ordering::SeqCst), "the renderer saw the flag");

    let r = scripted();
    r.tell.send(Step::Finish(made())).unwrap();
    until(&r.job, &JobState::Done(made()));
    r.job.cancel();
    std::thread::sleep(Duration::from_millis(60));
    assert_eq!(r.job.state(), JobState::Done(made()));

    // A cancel that arrives as the renderer finishes (it had made the video by then): the video is there, so it is said.
    let r = scripted();
    r.tell.send(Step::FinishWhenCancelled(made())).unwrap();
    std::thread::sleep(Duration::from_millis(60));
    r.job.cancel();
    until(&r.job, &JobState::Done(made()));
}

/// A job that is let go (the window closed, the recording deleted) is cancelled and waited for: nothing keeps
/// running behind the app, and the wait is as long as the renderer's own look at the flag, not as long as the work.
///
/// Mutation: a job that is let go and left running; one that is let go without being waited for.
#[test]
fn a_job_that_is_let_go_is_cancelled_and_waited_for() {
    let r = scripted();
    r.tell.send(Step::Progress(0.1)).unwrap();
    let (saw_cancel, tell) = (Arc::clone(&r.saw_cancel), r.tell);
    let from = Instant::now();
    drop(r.job);
    assert!(from.elapsed() < Duration::from_secs(5), "the wait for it was short");
    assert!(saw_cancel.load(Ordering::SeqCst), "the renderer was told to stop, and had stopped when the drop returned");
    drop(tell);
}

/// A folder of programs: where the named program is found. The first place that has it as a file that can be run
/// wins, in the order given (the search path first, then where a Mac's package manager puts it, which a program
/// started from the Finder does not have on its path); a file that cannot be run, a folder of that name and a place
/// that is not there are passed over.
///
/// Mutation: a file that cannot be run taken; the order reversed; a folder taken.
#[cfg(unix)]
#[test]
fn a_program_is_found_in_the_first_place_that_has_it_as_something_that_can_be_run() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let base = std::env::temp_dir().join(format!("azreview-job-find-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let make = |dir: &str, mode: u32| {
        let d = base.join(dir);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("ffmpeg"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(d.join("ffmpeg"), fs::Permissions::from_mode(mode)).unwrap();
        d
    };
    let plain = make("plain", 0o644);
    let first = make("first", 0o755);
    let second = make("second", 0o755);
    let folder = base.join("folder");
    fs::create_dir_all(folder.join("ffmpeg")).unwrap();
    let missing = base.join("not-there");
    let places = [missing.clone(), plain.clone(), folder.clone(), first.clone(), second.clone()];
    assert_eq!(find_program("ffmpeg", &places), Some(first.join("ffmpeg")));
    assert_eq!(find_program("ffmpeg", &[second.clone(), first.clone()]), Some(second.join("ffmpeg")), "in the order given");
    assert_eq!(find_program("ffmpeg", &[missing, plain, folder]), None, "nothing that can be run");
    assert_eq!(find_program("ffprobe", &[first]), None);
    let _ = fs::remove_dir_all(&base);
}

/// The places to look are the search path's folders, then the usual homes of a package manager's programs; a
/// search path that is not set is no places of its own.
///
/// Mutation: the usual homes left out; the search path after them.
#[test]
fn the_places_to_look_are_the_search_path_and_then_the_usual_homes() {
    let places = program_places(Some(std::ffi::OsString::from("/a/bin:/b/bin")));
    assert_eq!(&places[..2], [PathBuf::from("/a/bin"), PathBuf::from("/b/bin")]);
    assert!(places.contains(&PathBuf::from("/opt/homebrew/bin")) && places.contains(&PathBuf::from("/usr/local/bin")));
    assert!(places.iter().position(|p| p == Path::new("/b/bin")) < places.iter().position(|p| p == Path::new("/opt/homebrew/bin")));
    let none = program_places(None);
    assert!(none.contains(&PathBuf::from("/opt/homebrew/bin")), "{none:?}");
}

/// The program that is run: the one the settings name when it can be run; otherwise the first one found, with a
/// note that names what was asked for and what is used instead (a wrong path is never silently another program);
/// nothing named is no note; nothing anywhere is none.
///
/// Mutation: a named program that is used though it cannot be run; a fallback without its note; a note that does not
/// say which program was used.
#[cfg(unix)]
#[test]
fn the_program_named_is_used_when_it_can_be_run_and_else_the_first_found_with_a_note() {
    use std::{fs, os::unix::fs::PermissionsExt};
    let base = std::env::temp_dir().join(format!("azreview-job-choose-{}", std::process::id()));
    let _ = fs::remove_dir_all(&base);
    let make = |dir: &str, mode: u32| {
        let d = base.join(dir);
        fs::create_dir_all(&d).unwrap();
        fs::write(d.join("ffmpeg"), "#!/bin/sh\n").unwrap();
        fs::set_permissions(d.join("ffmpeg"), fs::Permissions::from_mode(mode)).unwrap();
        d.join("ffmpeg")
    };
    let named = make("named", 0o755);
    let broken = make("broken", 0o644);
    let found = make("found", 0o755);
    let places = [found.parent().unwrap().to_path_buf()];
    assert_eq!(choose_program("ffmpeg", Some(&named), &places), Some((named.clone(), None)));
    let (used, note) = choose_program("ffmpeg", Some(&broken), &places).unwrap();
    assert_eq!(used, found);
    let note = note.expect("a program other than the one asked for is said");
    assert!(note.contains(&broken.display().to_string()) && note.contains(&found.display().to_string()), "{note}");
    assert_eq!(choose_program("ffmpeg", None, &places), Some((found.clone(), None)));
    assert_eq!(choose_program("ffmpeg", Some(&named), &[]), Some((named, None)), "what is named needs no search");
    assert_eq!(choose_program("ffmpeg", Some(&broken), &[]), None);
    assert_eq!(choose_program("ffmpeg", None, &[]), None);
    let _ = fs::remove_dir_all(&base);
}

