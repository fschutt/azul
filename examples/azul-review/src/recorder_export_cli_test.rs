//! The export from the command line: how the arguments are read, and what it prints and returns for a renderer that
//! makes the video, one that fails and one that is cancelled. On a scripted renderer; the real one (ffmpeg) is in the
//! end-to-end tests of `recorder_export_test.rs`.

use std::{path::Path, sync::atomic::AtomicBool};

use super::*;
use crate::recorder_export_job::Made;

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(ToString::to_string).collect()
}

/// The recording to export, and where to put the video and which ffmpeg to use when they are named; the arguments
/// may come in any order.
///
/// Mutation: a switch whose value is read as the recording; the last of two recordings taken; a switch that is
/// not read.
#[test]
fn the_recording_and_the_switches_are_read_in_any_order() {
    assert_eq!(parse_args(args(&["/rec/a"])), Ok(Command::Export(Cli { session: PathBuf::from("/rec/a"), out: None, ffmpeg: None })));
    assert_eq!(
        parse_args(args(&["--out", "/v/a.mp4", "/rec/a", "--ffmpeg", "/bin/ffmpeg"])),
        Ok(Command::Export(Cli { session: PathBuf::from("/rec/a"), out: Some(PathBuf::from("/v/a.mp4")), ffmpeg: Some(PathBuf::from("/bin/ffmpeg")) }))
    );
    assert_eq!(parse_args(args(&["--help"])), Ok(Command::Help));
    assert_eq!(parse_args(args(&["/rec/a", "-h"])), Ok(Command::Help));
}

/// What cannot be read is refused in words that name it and say how it is used: no recording, two recordings, a switch
/// that is not one, a switch without its value.
///
/// Mutation: a refusal without the usage; a second recording taken; a switch without a value taken.
#[test]
fn what_cannot_be_read_is_refused_with_the_usage() {
    for bad in [args(&[]), args(&["/a", "/b"]), args(&["/a", "--speed", "2"]), args(&["/a", "--out"]), args(&["--ffmpeg"]), args(&["/a", "--out", "--ffmpeg", "/f"])] {
        let why = parse_args(bad.clone()).unwrap_err();
        assert!(why.contains("recorder_export SESSION"), "{bad:?}: {why}");
    }
    assert!(parse_args(args(&["/a", "--speed", "2"])).unwrap_err().contains("--speed"));
    assert!(parse_args(args(&["/a", "/b"])).unwrap_err().contains("/b"));
    assert!(parse_args(args(&["/a", "--out"])).unwrap_err().contains("--out needs a value"));
    assert!(parse_args(args(&["/a", "--out", "--ffmpeg", "/f"])).unwrap_err().contains("--out needs a value"), "a switch is no value");
}

/// A renderer that reports what it is given and returns what it is given.
struct Scripted {
    progress: Vec<f64>,
    result: Result<Made, Failure>,
    saw: Option<(PathBuf, bool)>,
}

impl Renderer for Scripted {
    fn render(&mut self, session: &Path, cancel: &AtomicBool, progress: &mut dyn FnMut(f64)) -> Result<Made, Failure> {
        self.saw = Some((session.to_path_buf(), cancel.load(std::sync::atomic::Ordering::SeqCst)));
        for f in &self.progress {
            progress(*f);
        }
        self.result.clone()
    }
}

fn made() -> Made {
    Made { file: PathBuf::from("/rec/a/export/video.mp4"), bytes: 4_321_000, notes: vec![String::from("a rounded camera came out as a rectangle")] }
}

fn run_with(cli: &Cli, scripted: &mut Scripted) -> (i32, String) {
    let mut out = Vec::new();
    let code = run_cli(cli, scripted, &mut out);
    (code, String::from_utf8(out).unwrap())
}

fn cli() -> Cli {
    Cli { session: PathBuf::from("/rec/a"), out: None, ffmpeg: None }
}

/// A video that was made: the progress in whole percents, each once and only when it changed, then where the video is
/// and how big, then what did not work as asked; nothing else, the exit code 0.
///
/// Mutation: a percent printed twice; one rounded up; one over 100; a progress that is not printed; the notes left out;
/// a success that exits 1.
#[test]
fn a_video_that_was_made_is_said_with_its_progress_its_place_and_its_notes() {
    let mut s = Scripted { progress: vec![0.0, 0.004, 0.2, 0.2004, 0.5, 1.0, 1.5], result: Ok(made()), saw: None };
    let (code, text) = run_with(&cli(), &mut s);
    assert_eq!(code, 0);
    assert_eq!(
        text.lines().collect::<Vec<_>>(),
        vec!["EXPORT_PROGRESS 0", "EXPORT_PROGRESS 20", "EXPORT_PROGRESS 50", "EXPORT_PROGRESS 100", "EXPORT_NOTE a rounded camera came out as a rectangle", "EXPORT_DONE /rec/a/export/video.mp4 4321000"]
    );
    assert_eq!(s.saw, Some((PathBuf::from("/rec/a"), false)), "the renderer is given the recording and a cancel that is not set");
}

/// A failure is said in its own words, on one line, with the exit code 1; a cancel is said and has the code 130.
///
/// Mutation: a failure that exits 0; a failure that loses its words; a cancel said as a failure.
#[test]
fn a_failure_and_a_cancel_are_said_and_have_their_own_exit_codes() {
    let mut s = Scripted { progress: vec![0.3], result: Err(Failure::Failed(String::from("ffmpeg failed at run1 (exit 1): no such file\nsecond line"))), saw: None };
    let (code, text) = run_with(&cli(), &mut s);
    assert_eq!(code, 1);
    assert_eq!(text.lines().last(), Some("EXPORT_FAILED ffmpeg failed at run1 (exit 1): no such file second line"), "{text}");
    let mut s = Scripted { progress: vec![], result: Err(Failure::Cancelled), saw: None };
    let (code, text) = run_with(&cli(), &mut s);
    assert_eq!(code, 130);
    assert_eq!(text.trim(), "EXPORT_CANCELLED");
}

/// A recording that is not there is said before anything is run: the renderer is not asked.
///
/// Mutation: a missing folder handed to the renderer.
#[test]
fn a_recording_that_is_not_a_folder_with_notes_is_refused_before_anything_runs() {
    let mut s = Scripted { progress: vec![], result: Ok(made()), saw: None };
    let mut out = Vec::new();
    let code = export_checked(&Cli { session: PathBuf::from("/no/such/recording"), out: None, ffmpeg: None }, &mut s, &mut out);
    let text = String::from_utf8(out).unwrap();
    assert_eq!(code, 1);
    assert!(text.starts_with("EXPORT_FAILED ") && text.contains("/no/such/recording") && text.contains("manifest.json"), "{text}");
    assert!(s.saw.is_none(), "the renderer was not asked");
}
