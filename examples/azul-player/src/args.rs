//! AzPlayer's command line: the switches every Azlin app understands (azul-appkit's `--screen`,
//! `--size`, `--theme`, `--mode`, `--shot`, `--shot-delay-ms`, `--sample`, `--data-dir`) with
//! AzPlayer's screens, plus its own: the folders the libraries are read from. Every one is a
//! switch, never an environment variable (user ruling 2026-10-07: "make all the variables cli
//! args"). Plain Rust, tested without a window.

use std::path::PathBuf;

use azul_appkit::{args::help, AppArgs, AppSpec};

/// The screens `--screen` opens: the start strip, a library, what plays, the settings.
pub const SCREENS: [&str; 9] = [
    "start",
    "music",
    "pictures",
    "videos",
    "movies",
    "tv",
    "recent",
    "now-playing",
    "settings",
];

/// What azul-appkit is told about AzPlayer.
pub const SPEC: AppSpec = AppSpec {
    name: "AzPlayer",
    binary: "AzPlayer",
    summary: "a media center for music, pictures and videos (MP4 / MOV with H.264 and AAC)",
    screens: &SCREENS,
    files_help: "the video files to open (the first plays)",
};

/// AzPlayer's own switches, after appkit's in the usage text.
const FOLDER_HELP: &str = "\nAZPLAYER (the folders the libraries read; default: the user's own):\n    \
    --music-dir <DIR>        The music library (default ~/Music)\n    \
    --pictures-dir <DIR>     The picture library (default ~/Pictures)\n    \
    --videos-dir <DIR>       The video and movie library (default ~/Movies on macOS, ~/Videos)\n    \
    --tv-dir <DIR>           Recorded TV (default: \"Recorded TV\" in the videos folder)\n";

/// The library folders asked for on the command line (`None` = the user's own).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct FolderArgs {
    pub music: Option<PathBuf>,
    pub pictures: Option<PathBuf>,
    pub videos: Option<PathBuf>,
    pub tv: Option<PathBuf>,
}

/// The parsed command line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    pub folders: FolderArgs,
    /// The switches every Azlin app understands (azul-appkit).
    pub kit: AppArgs,
}

/// The usage text: appkit's, then AzPlayer's folders.
#[must_use]
pub fn usage() -> String {
    let mut text = help(&SPEC);
    text.push_str(FOLDER_HELP);
    text
}

impl Args {
    /// Parses `argv` without the program name: the folder switches here, the rest by
    /// azul-appkit. `Err` carries the usage (`-h`) or what was wrong.
    pub fn parse<I, S>(argv: I) -> Result<Args, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        if argv.iter().any(|a| a == "-h" || a == "--help") {
            return Err(usage());
        }
        let mut folders = FolderArgs::default();
        let mut rest = Vec::with_capacity(argv.len());
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (arg, None),
            };
            let slot = match name {
                "--music-dir" => Some(&mut folders.music),
                "--pictures-dir" => Some(&mut folders.pictures),
                "--videos-dir" => Some(&mut folders.videos),
                "--tv-dir" => Some(&mut folders.tv),
                _ => None,
            };
            match slot {
                Some(slot) => {
                    let value = match inline {
                        Some(v) => v,
                        None => {
                            i += 1;
                            argv.get(i)
                                .cloned()
                                .ok_or_else(|| format!("{name} needs a folder"))?
                        }
                    };
                    if value.trim().is_empty() {
                        return Err(format!("{name} needs a folder"));
                    }
                    *slot = Some(PathBuf::from(value));
                }
                None => rest.push(argv[i].clone()),
            }
            i += 1;
        }
        let kit = AppArgs::parse(&SPEC, rest)?;
        Ok(Args { folders, kit })
    }

    /// The switches of this process (`std::env::args`, without the program name).
    pub fn from_env() -> Result<Args, String> {
        Args::parse(std::env::args().skip(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn the_folder_switches_are_read_in_both_spellings_and_the_rest_goes_to_the_kit() {
        let a = parse(&[
            "--music-dir",
            "/m",
            "--pictures-dir=/p",
            "--videos-dir",
            "/v",
            "--tv-dir=/t",
            "--theme",
            "flora",
            "movie.mp4",
        ])
        .unwrap();
        assert_eq!(a.folders.music, Some(PathBuf::from("/m")));
        assert_eq!(a.folders.pictures, Some(PathBuf::from("/p")));
        assert_eq!(a.folders.videos, Some(PathBuf::from("/v")));
        assert_eq!(a.folders.tv, Some(PathBuf::from("/t")));
        assert!(a.kit.theme.is_some(), "the kit's switches still work");
        assert_eq!(a.kit.files, vec![PathBuf::from("movie.mp4")]);
    }

    #[test]
    fn no_switch_is_the_users_own_folders_and_the_start_screen() {
        let a = parse(&[]).unwrap();
        assert_eq!(a.folders, FolderArgs::default());
        assert_eq!(a.kit.screen_or_default(&SPEC), "start");
    }

    #[test]
    fn a_folder_switch_without_a_folder_is_an_error() {
        assert!(parse(&["--music-dir"]).is_err());
        assert!(parse(&["--videos-dir="]).is_err());
        assert!(parse(&["--nonsense"]).is_err(), "unknown switches are still refused");
    }

    #[test]
    fn the_screens_and_the_help_name_every_switch() {
        assert_eq!(
            parse(&["--screen", "music"]).unwrap().kit.screen.as_deref(),
            Some("music")
        );
        assert!(parse(&["--screen", "radio"]).is_err());
        let help = parse(&["--help"]).unwrap_err();
        for name in ["--music-dir", "--pictures-dir", "--videos-dir", "--tv-dir", "--screen"] {
            assert!(help.contains(name), "the usage names {name}");
        }
    }
}
