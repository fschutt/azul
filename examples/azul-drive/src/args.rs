//! AzDrive's command line: the switches every Azlin app understands (azul-appkit's
//! `--screen`, `--size`, `--theme`, `--mode`, `--shot`, `--sample`, `--data-dir`) with
//! AzDrive's screens (`this-pc | quick-access | home | settings`), plus its own `--layout <name>`,
//! `--home <dir>`, `--downloads <dir>`, `--drives <file>`, `--dialogs <window|inline>` and
//! `--cache-dir <dir>`.
//! `--sample` writes the sample files into the Home drive.
//!
//! Every setting is a flag; the environment variables of older builds are read only when their
//! flag is absent ([`Args::with_env_fallbacks`]): `--home` / `$AZDRIVE_HOME`, `--downloads` /
//! `$AZDRIVE_DOWNLOADS`, `--drives` / `$AZUL_DRIVES` (azul-storage's, shared with AzMail),
//! `--dialogs` / `$AZDRIVE_DIALOGS`, `--cache-dir` / `$AZDRIVE_CACHE`, `--data-dir` /
//! `$AZLIN_DATA` (azul-appkit's).

use std::path::{Path, PathBuf};

use azul_appkit::{args::help, AppArgs, AppSpec};
use azul_storage::{config::DRIVES_VAR, Drive, LocalDrive};

use crate::model::ViewLayout;

/// What `--home` falls back to: the folder the Home drive shows.
pub const HOME_VAR: &str = "AZDRIVE_HOME";
/// What `--downloads` falls back to: where Download saves.
pub const DOWNLOADS_VAR: &str = "AZDRIVE_DOWNLOADS";
/// What `--dialogs` falls back to (`inline`: the dialogs as sheets inside the window).
pub const DIALOGS_VAR: &str = "AZDRIVE_DIALOGS";
/// What `--cache-dir` falls back to: where the caches are kept.
pub const CACHE_VAR: &str = "AZDRIVE_CACHE";

/// How the dialogs (Add drive, Properties, the conflicts, ...) show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Dialogs {
    /// A modal dialog window of its own.
    #[default]
    Window,
    /// A sheet inside the main window (the scripts: the debug server drives the main window).
    Inline,
}

impl Dialogs {
    /// `window` or `inline`.
    #[must_use]
    pub fn from_name(name: &str) -> Option<Dialogs> {
        match name.trim() {
            "window" => Some(Dialogs::Window),
            "inline" => Some(Dialogs::Inline),
            _ => None,
        }
    }
}

/// Where the window opens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// Where the settings say (This PC or Quick access).
    #[default]
    Default,
    ThisPc,
    QuickAccess,
    /// The Home drive's root.
    Home,
    /// The backstage with the Options.
    Settings,
}

/// The `--screen` names, in [`Screen`] order after `Default`.
const SCREENS: [(&str, Screen); 4] = [
    ("this-pc", Screen::ThisPc),
    ("quick-access", Screen::QuickAccess),
    ("home", Screen::Home),
    ("settings", Screen::Settings),
];

/// What azul-appkit is told about AzDrive.
pub const SPEC: AppSpec = AppSpec {
    name: "AzDrive",
    binary: "AzDrive",
    summary: "a file manager like Windows Explorer, for local folders and S3 drives",
    screens: &["this-pc", "quick-access", "home", "settings"],
    files_help: "",
};

/// AzDrive's own switches, after appkit's in the usage text.
const AZDRIVE_HELP: &str = concat!(
    "\nAZDRIVE:\n",
    "    --layout <NAME>          extra-large-icons | large-icons | medium-icons | small-icons |\n",
    "                             list | details | tiles | content\n",
    "    --home <DIR>             The folder the Home drive shows (default: $AZDRIVE_HOME, else\n",
    "                             your home folder)\n",
    "    --downloads <DIR>        Where Download saves (default: $AZDRIVE_DOWNLOADS, else your\n",
    "                             Downloads folder)\n",
    "    --drives <FILE>          The drives file, shared with AzMail (default: $AZUL_DRIVES,\n",
    "                             else <config dir>/azul-storage/drives.json)\n",
    "    --dialogs <HOW>          window | inline: the dialogs as windows, or as sheets inside\n",
    "                             the window (default: $AZDRIVE_DIALOGS, else window)\n",
    "    --cache-dir <DIR>        Where a cloud drive's last listing and the drives' search\n",
    "                             indexes are kept (default: $AZDRIVE_CACHE, else\n",
    "                             <cache dir>/AzDrive; none in a --shot run)\n",
    "    --open <PATH>            Open at a place as the address bar names it: This PC,\n",
    "                             Home/Documents (File > Open new window passes it)\n",
    "    --token-url <URL>        The Azlin token server of Add drive > Buy storage (default:\n",
    "                             $AZLIN_TOKEN_URL, else the shared Azlin config's endpoints,\n",
    "                             else the profile's)\n",
    "    --profile <NAME>         local | trial | production: whose addresses are the defaults\n",
    "                             (default: $AZLIN_PROFILE, else the shared config's, else local)\n",
);

/// AzDrive's own switches that take a value (after a space or an equals sign), and what the
/// value is called in an error.
const OWN: [(&str, &str); 9] = [
    ("--layout", "name"),
    ("--home", "folder"),
    ("--downloads", "folder"),
    ("--drives", "file"),
    ("--dialogs", "window or inline"),
    ("--open", "place such as Home/Documents"),
    ("--token-url", "address such as https://token.example"),
    ("--profile", "profile: local, trial or production"),
    ("--cache-dir", "folder"),
];

/// The parsed command line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    pub screen: Screen,
    /// The layout to open folders in (instead of the saved one).
    pub layout: Option<ViewLayout>,
    /// `--home`: the folder the Home drive shows (the user's home when absent).
    pub home: Option<PathBuf>,
    /// `--downloads`: where Download saves (the user's Downloads folder when absent).
    pub downloads: Option<PathBuf>,
    /// `--drives`: the drives file (`<config dir>/azul-storage/drives.json` when absent).
    pub drives: Option<PathBuf>,
    /// `--dialogs`: dialog windows or inline sheets (windows when absent).
    pub dialogs: Option<Dialogs>,
    /// `--open`: the place to open at, as the address bar names it (`Home/Documents`); it wins
    /// over `--screen`.
    pub open: Option<String>,
    /// `--token-url`: the Azlin token server (over the environment and the shared config;
    /// azul-appkit's `resolve_endpoints` weighs them).
    pub token_url: Option<String>,
    /// `--profile`: the Azlin profile whose addresses are the defaults.
    pub profile: Option<String>,
    /// `--cache-dir`: where the caches are kept (`<cache dir>/AzDrive` when absent).
    pub cache_dir: Option<PathBuf>,
    /// The switches every Azlin app understands (azul-appkit): `--theme`, `--mode`
    /// (`system` too), `--size`, `--shot`, `--sample`, `--data-dir`.
    pub kit: AppArgs,
}

/// The usage text: appkit's, then AzDrive's own switches.
#[must_use]
pub fn usage() -> String {
    let mut text = help(&SPEC);
    text.push_str(AZDRIVE_HELP);
    text
}

/// A folder or file a switch names; `Err` for an empty one.
fn path_value(name: &str, what: &str, value: &str) -> Result<PathBuf, String> {
    let value = value.trim();
    if value.is_empty() {
        return Err(format!("{name} needs a {what}"));
    }
    Ok(PathBuf::from(value))
}

impl Args {
    /// Parses `argv` without the program name: AzDrive's own switches here, the rest by
    /// azul-appkit. `Err` carries the usage (`-h`) or what was wrong. The environment is not
    /// read here ([`Args::with_env_fallbacks`]).
    pub fn parse<I, S>(argv: I) -> Result<Args, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        if argv.iter().any(|a| a == "-h" || a == "--help") {
            return Err(usage());
        }
        let mut args = Args::default();
        let mut rest = Vec::with_capacity(argv.len());
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (arg, None),
            };
            let Some(&(name, what)) = OWN.iter().find(|(n, _)| *n == name) else {
                rest.push(argv[i].clone());
                i += 1;
                continue;
            };
            let value = match inline {
                Some(v) => v,
                None => {
                    i += 1;
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| format!("{name} needs a {what}"))?
                }
            };
            match name {
                "--layout" => {
                    args.layout = Some(ViewLayout::from_name(&value).ok_or_else(|| {
                        format!(
                            "--layout: expected large-icons, list, details, tiles, ... got \
                             {value:?}"
                        )
                    })?);
                }
                "--home" => args.home = Some(path_value(name, what, &value)?),
                "--downloads" => args.downloads = Some(path_value(name, what, &value)?),
                "--drives" => args.drives = Some(path_value(name, what, &value)?),
                "--cache-dir" => args.cache_dir = Some(path_value(name, what, &value)?),
                "--open" => {
                    let place = value.trim();
                    if place.is_empty() {
                        return Err(format!("{name} needs a {what}"));
                    }
                    args.open = Some(place.to_string());
                }
                "--token-url" | "--profile" => {
                    let value = value.trim();
                    if value.is_empty() {
                        return Err(format!("{name} needs a {what}"));
                    }
                    if name == "--token-url" {
                        args.token_url = Some(value.to_string());
                    } else {
                        args.profile = Some(value.to_string());
                    }
                }
                _ => {
                    args.dialogs = Some(Dialogs::from_name(&value).ok_or_else(|| {
                        format!("--dialogs: expected window or inline, got {value:?}")
                    })?);
                }
            }
            i += 1;
        }
        args.kit = AppArgs::parse(&SPEC, rest)?;
        args.screen = args
            .kit
            .screen
            .as_deref()
            .and_then(|name| SCREENS.iter().find(|(n, _)| *n == name))
            .map_or(Screen::Default, |(_, s)| *s);
        Ok(args)
    }

    /// The switches, each absent one filled from the environment variable an older build read
    /// (`env` reads a variable; `std::env::var` in the app): `--home` from `$AZDRIVE_HOME`,
    /// `--downloads` from `$AZDRIVE_DOWNLOADS`, `--drives` from `$AZUL_DRIVES`, `--dialogs` from
    /// `$AZDRIVE_DIALOGS`, `--cache-dir` from `$AZDRIVE_CACHE`. A switch given always wins; an
    /// empty variable counts as unset.
    #[must_use]
    pub fn with_env_fallbacks(mut self, env: impl Fn(&str) -> Option<String>) -> Args {
        let path = |var: &str| {
            env(var)
                .map(|v| v.trim().to_string())
                .filter(|v| !v.is_empty())
                .map(PathBuf::from)
        };
        if self.home.is_none() {
            self.home = path(HOME_VAR);
        }
        if self.downloads.is_none() {
            self.downloads = path(DOWNLOADS_VAR);
        }
        if self.drives.is_none() {
            self.drives = path(DRIVES_VAR);
        }
        if self.dialogs.is_none() {
            self.dialogs = env(DIALOGS_VAR).and_then(|v| Dialogs::from_name(&v));
        }
        if self.cache_dir.is_none() {
            self.cache_dir = path(CACHE_VAR);
        }
        self
    }
}

/// The switches of another window of this run (File > Open new window): this run's, without
/// the ones that only made sense once (`--screen`, `--shot`, `--sample`, an earlier `--open`),
/// then `--open <path>`. `argv` is this run's, without the program name.
#[must_use]
pub fn new_window_args<I, S>(argv: I, path: &str) -> Vec<String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    /// Switches with a value that the new window does not take over.
    const ONCE: [&str; 3] = ["--screen", "--shot", "--open"];
    let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
    let mut out = Vec::with_capacity(argv.len() + 2);
    let mut i = 0;
    while i < argv.len() {
        let arg = argv[i].as_str();
        let name = arg.split_once('=').map_or(arg, |(n, _)| n);
        if arg == "--sample" {
            i += 1;
            continue;
        }
        if ONCE.contains(&name) {
            // `--name value` drops its value too; `--name=value` is one argument.
            i += if arg.contains('=') { 1 } else { 2 };
            continue;
        }
        out.push(argv[i].clone());
        i += 1;
    }
    out.push(String::from("--open"));
    out.push(path.to_string());
    out
}

/// A 16x16 gradient, the sample picture.
const SAMPLE_PNG: &[u8] = &[
    137, 80, 78, 71, 13, 10, 26, 10, 0, 0, 0, 13, 73, 72, 68, 82, 0, 0, 0, 16, 0, 0, 0, 16, 8,
    2, 0, 0, 0, 144, 145, 104, 54, 0, 0, 1, 192, 73, 68, 65, 84, 120, 218, 13, 203, 161, 193,
    70, 64, 0, 0, 208, 127, 156, 111, 0, 3, 24, 192, 0, 6, 48, 128, 40, 8, 226, 69, 65, 16, 47,
    10, 130, 120, 81, 16, 94, 188, 40, 8, 162, 104, 132, 223, 235, 239, 239, 215, 41, 58, 101,
    167, 234, 212, 157, 166, 211, 118, 250, 78, 232, 76, 157, 216, 89, 59, 169, 115, 116, 114,
    231, 234, 252, 253, 6, 197, 160, 28, 84, 131, 122, 208, 12, 218, 65, 63, 8, 131, 105, 16,
    7, 235, 32, 13, 142, 65, 30, 92, 195, 23, 130, 34, 40, 131, 42, 168, 131, 38, 104, 131, 62,
    8, 193, 20, 196, 96, 13, 82, 112, 4, 57, 184, 194, 23, 70, 197, 168, 28, 85, 163, 122, 212,
    140, 218, 81, 63, 10, 163, 105, 20, 71, 235, 40, 141, 142, 81, 30, 93, 227, 23, 102, 197,
    172, 156, 85, 179, 122, 214, 204, 218, 89, 63, 11, 179, 105, 22, 103, 235, 44, 205, 142,
    89, 158, 93, 243, 23, 162, 34, 42, 163, 42, 170, 163, 38, 106, 163, 62, 10, 209, 20, 197,
    104, 141, 82, 116, 68, 57, 186, 226, 23, 22, 197, 162, 92, 84, 139, 122, 209, 44, 218, 69,
    191, 8, 139, 105, 17, 23, 235, 34, 45, 142, 69, 94, 92, 203, 23, 54, 197, 166, 220, 84,
    155, 122, 211, 108, 218, 77, 191, 9, 155, 105, 19, 55, 235, 38, 109, 142, 77, 222, 92, 219,
    23, 146, 34, 41, 147, 42, 169, 147, 38, 105, 147, 62, 9, 201, 148, 196, 100, 77, 82, 114,
    36, 57, 185, 210, 23, 118, 197, 174, 220, 85, 187, 122, 215, 236, 218, 93, 191, 11, 187,
    105, 23, 119, 235, 46, 237, 142, 93, 222, 93, 251, 23, 40, 40, 169, 168, 105, 104, 233, 9,
    76, 68, 86, 18, 7, 153, 203, 23, 178, 34, 43, 179, 42, 171, 179, 38, 107, 179, 62, 11, 217,
    148, 197, 108, 205, 82, 118, 100, 57, 187, 242, 23, 78, 197, 169, 60, 85, 167, 250, 212,
    156, 218, 83, 127, 10, 167, 233, 20, 79, 235, 41, 157, 142, 83, 62, 93, 231, 23, 110, 197,
    173, 188, 85, 183, 250, 214, 220, 218, 91, 127, 11, 183, 233, 22, 111, 235, 45, 221, 142,
    91, 190, 93, 247, 23, 30, 197, 163, 124, 84, 143, 250, 209, 60, 218, 71, 255, 8, 143, 233,
    17, 31, 235, 35, 61, 142, 71, 126, 92, 207, 23, 94, 197, 171, 124, 85, 175, 250, 213, 188,
    218, 87, 255, 10, 175, 233, 21, 95, 235, 43, 189, 142, 87, 126, 93, 175, 127, 81, 183, 254,
    16, 120, 157, 103, 194, 0, 0, 0, 0, 73, 69, 78, 68, 174, 66, 96, 130,
];

/// A short fading 440 Hz tone as a 16-bit mono WAV: the sample sound.
#[allow(clippy::cast_possible_truncation, clippy::cast_precision_loss)]
fn chime_wav() -> Vec<u8> {
    const RATE: u32 = 8_000;
    const COUNT: u32 = RATE * 3 / 10;
    const SECONDS: f32 = COUNT as f32 / RATE as f32;
    let data: Vec<u8> = (0..COUNT)
        .flat_map(|i| {
            let t = i as f32 / RATE as f32;
            let fade = 1.0 - t / SECONDS;
            let sample = ((t * 440.0 * std::f32::consts::TAU).sin() * fade * 12_000.0) as i16;
            sample.to_le_bytes()
        })
        .collect();
    let mut out = Vec::with_capacity(44 + data.len());
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data.len() as u32).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes());
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&RATE.to_le_bytes());
    out.extend_from_slice(&(RATE * 2).to_le_bytes());
    out.extend_from_slice(&2u16.to_le_bytes());
    out.extend_from_slice(&16u16.to_le_bytes());
    out.extend_from_slice(b"data");
    out.extend_from_slice(&(data.len() as u32).to_le_bytes());
    out.extend_from_slice(&data);
    out
}

/// The sample files: (key, bytes). A key ending in `/` is an empty folder.
fn sample_files() -> Vec<(&'static str, Vec<u8>)> {
    vec![
        (
            "Documents/notes.txt",
            b"AzDrive sample notes\n\nF2 renames, Ctrl+C / Ctrl+V copy, Delete moves to the trash \
              folder,\nCtrl+Z brings it back.\n"
                .to_vec(),
        ),
        (
            "Documents/report.md",
            b"# Quarterly report\n\n| Month | Files |\n|-------|-------|\n| July  | 12    |\n"
                .to_vec(),
        ),
        (
            "Documents/data.csv",
            b"name,size,kind\nnotes.txt,120,text\ngradient.png,505,image\n".to_vec(),
        ),
        (
            "Code/main.rs",
            b"fn main() {\n    println!(\"Hello from AzDrive\");\n}\n".to_vec(),
        ),
        ("Pictures/gradient.png", SAMPLE_PNG.to_vec()),
        ("Music/chime.wav", chime_wav()),
        ("Archive/2024/old-notes.txt", b"Last year's notes.\n".to_vec()),
        (".hidden-settings", b"a hidden item\n".to_vec()),
        ("Projects/", Vec::new()),
    ]
}

/// Writes the sample files into the folder `home` through a `LocalDrive`
/// (never over a file that is there). Returns how many it wrote. The home
/// folder is not the data tree: no `.azlin/` bookkeeping is left there.
pub fn write_sample(home: &Path) -> Result<usize, String> {
    let drive = LocalDrive::without_manifest(home);
    let mut written = 0;
    for (key, bytes) in sample_files() {
        if key.ends_with('/') {
            drive.create_folder(key).map_err(|e| e.to_string())?;
            written += 1;
            continue;
        }
        if azul_storage::ops::exists(&drive, key).map_err(|e| e.to_string())? {
            continue;
        }
        drive.put(key, &bytes).map_err(|e| e.to_string())?;
        written += 1;
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ViewLayout;
    use azul_appkit::{ModePref, Theme};

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().map(|s| s.to_string()))
    }

    /// `--open` names the place as the address bar does; an empty one is an error.
    #[test]
    fn open_names_the_place_to_start_at() {
        let args = parse(&["--open", "Home/Documents"]).unwrap();
        assert_eq!(args.open.as_deref(), Some("Home/Documents"));
        assert_eq!(parse(&["--open=This PC"]).unwrap().open.as_deref(), Some("This PC"));
        assert!(parse(&["--open", " "]).is_err());
        assert_eq!(parse(&[]).unwrap().open, None);
    }

    /// A new window takes this run's switches, not the ones that were for this start only, and
    /// opens where the window it came from is.
    #[test]
    fn a_new_window_takes_the_runs_switches_and_opens_at_the_place() {
        let argv = [
            "--home",
            "/h",
            "--screen",
            "this-pc",
            "--shot=/tmp/a.png",
            "--sample",
            "--open",
            "Home",
            "--theme",
            "flora",
        ];
        assert_eq!(
            new_window_args(argv, "Home/Documents"),
            vec!["--home", "/h", "--theme", "flora", "--open", "Home/Documents"]
        );
        assert_eq!(new_window_args(Vec::<String>::new(), "This PC"), vec!["--open", "This PC"]);
    }

    #[test]
    fn no_switches_start_where_the_settings_say() {
        let args = parse(&[]).unwrap();
        assert_eq!(args.screen, Screen::Default);
        assert_eq!(args.kit.theme, None, "the settings file decides");
        assert_eq!(args.kit.mode, None);
        assert_eq!(args.layout, None);
        assert!(!args.kit.sample);
        assert_eq!(args.kit.data_dir, None);
    }

    #[test]
    fn every_switch_reads_its_value_with_a_space_or_an_equals_sign() {
        let args = parse(&[
            "--screen",
            "home",
            "--theme=flora",
            "--mode",
            "dark",
            "--layout",
            "large-icons",
            "--sample",
            "--data-dir",
            "/tmp/azlin",
        ])
        .unwrap();
        assert_eq!(args.screen, Screen::Home);
        assert_eq!(args.kit.theme, Some(Theme::Flora));
        assert_eq!(args.kit.mode, Some(ModePref::Dark));
        assert_eq!(args.layout, Some(ViewLayout::LargeIcons));
        assert!(args.kit.sample);
        assert_eq!(args.kit.data_dir, Some(std::path::PathBuf::from("/tmp/azlin")));
        assert_eq!(parse(&["--mode", "light"]).unwrap().kit.mode, Some(ModePref::Light));
        assert_eq!(
            parse(&["--mode", "system"]).unwrap().kit.mode,
            Some(ModePref::System),
            "the OS's mode, as in every Azlin app"
        );
        assert_eq!(parse(&["--layout=details"]).unwrap().layout, Some(ViewLayout::Details));
        assert_eq!(
            parse(&["--screen", "quick-access"]).unwrap().screen,
            Screen::QuickAccess
        );
        assert_eq!(
            parse(&["--screen=settings"]).unwrap().screen,
            Screen::Settings
        );
        assert_eq!(parse(&["--size", "900x600"]).unwrap().kit.size, Some((900.0, 600.0)));
    }

    #[test]
    fn a_wrong_switch_or_value_says_what_is_expected() {
        assert!(parse(&["--screen", "moon"]).unwrap_err().contains("this-pc"));
        assert!(parse(&["--theme", "neon"]).unwrap_err().contains("flora"));
        assert!(parse(&["--mode"]).unwrap_err().contains("--mode"));
        assert!(parse(&["--layout", "huge"]).unwrap_err().contains("details"));
        assert!(parse(&["--layout"]).unwrap_err().contains("--layout"));
        assert!(parse(&["--what"]).unwrap_err().contains("unknown"));
        let help = parse(&["--help"]).unwrap_err();
        assert!(help.contains("USAGE") && help.contains("--layout") && help.contains("--data-dir"));
    }

    #[test]
    fn the_home_the_downloads_the_drives_file_and_the_dialogs_are_switches() {
        let args = parse(&[
            "--home",
            "/tmp/home",
            "--downloads=/tmp/dl",
            "--drives",
            "/tmp/drives.json",
            "--dialogs",
            "inline",
        ])
        .unwrap();
        assert_eq!(args.home, Some(PathBuf::from("/tmp/home")));
        assert_eq!(args.downloads, Some(PathBuf::from("/tmp/dl")));
        assert_eq!(args.drives, Some(PathBuf::from("/tmp/drives.json")));
        assert_eq!(args.dialogs, Some(Dialogs::Inline));
        assert_eq!(parse(&["--dialogs=window"]).unwrap().dialogs, Some(Dialogs::Window));
        assert!(parse(&["--dialogs", "sheet"]).unwrap_err().contains("inline"));
        assert!(parse(&["--home"]).unwrap_err().contains("--home"));
        assert!(parse(&["--home="]).unwrap_err().contains("--home needs a folder"));
        assert_eq!(parse(&[]).unwrap().dialogs, None, "windows unless asked");
        let help = parse(&["-h"]).unwrap_err();
        for flag in ["--home", "--downloads", "--drives", "--dialogs", "$AZDRIVE_HOME", "$AZUL_DRIVES"]
        {
            assert!(help.contains(flag), "{flag} in the usage");
        }
    }

    #[test]
    fn a_switch_wins_over_its_environment_variable_which_fills_in_when_it_is_absent() {
        let env = |var: &str| match var {
            "AZDRIVE_HOME" => Some("/env/home".to_string()),
            "AZDRIVE_DOWNLOADS" => Some("  ".to_string()),
            "AZUL_DRIVES" => Some("/env/drives.json".to_string()),
            "AZDRIVE_DIALOGS" => Some("inline".to_string()),
            _ => None,
        };
        let args = parse(&["--home", "/flag/home"])
            .unwrap()
            .with_env_fallbacks(env);
        assert_eq!(args.home, Some(PathBuf::from("/flag/home")), "the switch wins");
        assert_eq!(args.downloads, None, "an empty variable names no folder");
        assert_eq!(args.drives, Some(PathBuf::from("/env/drives.json")));
        assert_eq!(args.dialogs, Some(Dialogs::Inline));
        let args = parse(&["--dialogs", "window"])
            .unwrap()
            .with_env_fallbacks(env);
        assert_eq!(args.dialogs, Some(Dialogs::Window), "the switch wins");
        assert_eq!(
            parse(&[]).unwrap().with_env_fallbacks(|_| None),
            parse(&[]).unwrap(),
            "no variable, nothing filled in"
        );
    }

    /// The folder AzDrive keeps its caches in (a cloud drive's last listing, the drives'
    /// indexes) is a switch, `$AZDRIVE_CACHE` filling in when it is absent.
    #[test]
    fn the_cache_folder_is_a_switch_with_its_variable() {
        let args = parse(&["--cache-dir", "/tmp/cache"]).unwrap();
        assert_eq!(args.cache_dir, Some(PathBuf::from("/tmp/cache")));
        assert_eq!(parse(&[]).unwrap().cache_dir, None, "the user's cache folder");
        assert!(parse(&["--cache-dir="]).unwrap_err().contains("--cache-dir needs a folder"));
        let env = |var: &str| (var == "AZDRIVE_CACHE").then(|| String::from("/env/cache"));
        let filled = parse(&[]).unwrap().with_env_fallbacks(env);
        assert_eq!(filled.cache_dir, Some(PathBuf::from("/env/cache")));
        let kept = parse(&["--cache-dir", "/flag"]).unwrap().with_env_fallbacks(env);
        assert_eq!(kept.cache_dir, Some(PathBuf::from("/flag")), "the switch wins");
        assert!(parse(&["-h"]).unwrap_err().contains("--cache-dir"));
    }

    #[test]
    fn the_sample_files_land_in_the_home_folder_and_a_second_run_keeps_them() {
        let dir = std::env::temp_dir().join(format!("azdrive-sample-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let written = write_sample(&dir).unwrap();
        assert!(written >= 6, "{written}");
        assert!(dir.join("Documents").join("notes.txt").is_file());
        assert!(dir.join("Pictures").join("gradient.png").is_file());
        assert!(dir.join("Projects").is_dir(), "an empty folder too");
        let chime = std::fs::read(dir.join("Music").join("chime.wav")).unwrap();
        let wav = crate::preview::wav_samples(&chime).expect("the sample sound is a WAV");
        assert_eq!((wav.sample_rate, wav.channels), (8_000, 1));
        assert_eq!(wav.samples.len(), 2_400);
        std::fs::write(dir.join("Documents").join("notes.txt"), b"mine").unwrap();
        write_sample(&dir).unwrap();
        assert_eq!(
            std::fs::read(dir.join("Documents").join("notes.txt")).unwrap(),
            b"mine",
            "the user's file is never overwritten"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }
}
