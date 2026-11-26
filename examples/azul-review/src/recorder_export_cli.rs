//! The export of a recording from the command line: `recorder_export SESSION [--out FILE] [--ffmpeg PATH]`. It is the
//! same call the window's Export button makes ([`crate::recorder_export_job::FfmpegRenderer`]) with a progress that is
//! printed instead of drawn, so the runners and the tests that try an export by hand use the code the app does.
//!
//! stdout, one line each: `EXPORT_PROGRESS <percent>` (whole percents, each once), `EXPORT_NOTE <what did not work as
//! asked>`, and last `EXPORT_DONE <file> <bytes>` (exit 0), `EXPORT_FAILED <why>` (exit 1) or `EXPORT_CANCELLED` (exit
//! 130). A wrong command line is said on stderr with the usage (exit 2).

use std::{
    io::Write,
    path::PathBuf,
    sync::atomic::AtomicBool,
};

use crate::{
    recorder_export_job::{Failure, FfmpegRenderer, Renderer},
    recorder_manifest::MANIFEST_FILE,
};

/// How the command is used, said with every refusal.
pub const USAGE: &str = "usage: recorder_export SESSION [--out FILE] [--ffmpeg PATH]
  SESSION   the folder of a recording (review/recordings/2026-11-26_10-00-00 of the data folder)
  --out     where to make the video (default: export/video.mp4 in the recording's folder)
  --ffmpeg  the program to use (default: the one on the search path, or where a package manager puts it)
stdout: EXPORT_PROGRESS <percent>, EXPORT_NOTE <text>, then EXPORT_DONE <file> <bytes> | EXPORT_FAILED <why> | EXPORT_CANCELLED";

/// What the command line asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cli {
    pub session: PathBuf,
    pub out: Option<PathBuf>,
    pub ffmpeg: Option<PathBuf>,
}

/// What to do.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Command {
    Export(Cli),
    Help,
}

/// The command line (without the program's name).
///
/// # Errors
/// What cannot be read, in words, with the usage.
pub fn parse_args(args: Vec<String>) -> Result<Command, String> {
    let refuse = |why: String| Err(format!("{why}\n{USAGE}"));
    let (mut session, mut out, mut ffmpeg) = (None, None, None);
    let mut args = args.into_iter();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-h" | "--help" => return Ok(Command::Help),
            "--out" | "--ffmpeg" => {
                let Some(value) = args.next().filter(|v| !v.starts_with("--")) else {
                    return refuse(format!("{arg} needs a value"));
                };
                if arg == "--out" {
                    out = Some(PathBuf::from(value));
                } else {
                    ffmpeg = Some(PathBuf::from(value));
                }
            }
            other if other.starts_with("--") => return refuse(format!("not understood: {other}")),
            _ if session.is_some() => return refuse(format!("one recording at a time: {arg} is a second")),
            _ => session = Some(PathBuf::from(arg)),
        }
    }
    match session {
        Some(session) => Ok(Command::Export(Cli { session, out, ffmpeg })),
        None => refuse(String::from("which recording? give its folder")),
    }
}

/// The whole percent a fraction is (never ahead of what was done).
fn percent(fraction: f64) -> u32 {
    (fraction.clamp(0.0, 1.0) * 100.0 + 1e-9).floor() as u32
}

/// Exports with `renderer`, saying what happens on `out`; the exit code.
pub fn run_cli(cli: &Cli, renderer: &mut dyn Renderer, out: &mut dyn Write) -> i32 {
    let mut last: Option<u32> = None;
    let cancel = AtomicBool::new(false);
    let result = renderer.render(&cli.session, &cancel, &mut |fraction| {
        let now = percent(fraction);
        if last != Some(now) {
            last = Some(now);
            let _ = writeln!(out, "EXPORT_PROGRESS {now}");
        }
    });
    match result {
        Ok(made) => {
            for note in &made.notes {
                let _ = writeln!(out, "EXPORT_NOTE {note}");
            }
            let _ = writeln!(out, "EXPORT_DONE {} {}", made.file.display(), made.bytes);
            0
        }
        Err(Failure::Cancelled) => {
            let _ = writeln!(out, "EXPORT_CANCELLED");
            130
        }
        Err(Failure::Failed(why)) => {
            let _ = writeln!(out, "EXPORT_FAILED {}", why.split_whitespace().collect::<Vec<_>>().join(" "));
            1
        }
    }
}

/// [`run_cli`] for a folder that is a recording's: one that is not is said before anything runs.
pub fn export_checked(cli: &Cli, renderer: &mut dyn Renderer, out: &mut dyn Write) -> i32 {
    if !cli.session.join(MANIFEST_FILE).is_file() {
        let _ = writeln!(out, "EXPORT_FAILED {} has no {MANIFEST_FILE}: it is not the folder of a recording", cli.session.display());
        return 1;
    }
    run_cli(cli, renderer, out)
}

/// What the `recorder_export` program does: reads its own command line, exports with ffmpeg, prints; its exit code.
#[must_use]
pub fn main_args() -> i32 {
    match parse_args(std::env::args().skip(1).collect()) {
        Err(why) => {
            eprintln!("{why}");
            2
        }
        Ok(Command::Help) => {
            println!("{USAGE}");
            0
        }
        Ok(Command::Export(cli)) => {
            let mut renderer = FfmpegRenderer { ffmpeg: cli.ffmpeg.clone(), preferred: None, out: cli.out.clone() };
            export_checked(&cli, &mut renderer, &mut std::io::stdout())
        }
    }
}

#[cfg(test)]
#[path = "recorder_export_cli_test.rs"]
mod recorder_export_cli_test;
