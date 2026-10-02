//! AzDrive's command line: `--screen <this-pc|quick-access|home|settings>`,
//! `--theme <flat|flora>`, `--mode <light|dark>`, `--layout <name>`,
//! `--sample` (sample files in the Home drive), like AzWriter's switches.

use std::path::Path;

use azul_storage::{Drive, LocalDrive};

use crate::model::ViewLayout;

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

/// The parsed command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub screen: Screen,
    /// The app theme (`flat`, `flora`).
    pub theme: Option<String>,
    /// The mode: `Some(true)` dark, `Some(false)` light, `None` the OS's.
    pub dark: Option<bool>,
    /// The layout to open folders in (instead of the saved one).
    pub layout: Option<ViewLayout>,
    /// Write the sample files into the Home drive first.
    pub sample: bool,
}

pub const HELP: &str = "\
AzDrive - a file manager like Windows Explorer, for local folders and S3 drives

USAGE:
    AzDrive [OPTIONS]

OPTIONS:
    --screen <NAME>    this-pc | quick-access | home | settings
    --theme <NAME>     flat | flora
    --mode <MODE>      light | dark (default: the system's)
    --layout <NAME>    extra-large-icons | large-icons | medium-icons | small-icons |
                       list | details | tiles | content
    --sample           write sample files into the Home drive (never over a file)
    -h, --help         print this help
";

impl Args {
    pub fn parse<I, S>(argv: I) -> Result<Args, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut args = Args::default();
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (arg, None),
            };
            let mut value = |what: &str| -> Result<String, String> {
                if let Some(v) = inline.clone() {
                    return Ok(v);
                }
                i += 1;
                argv.get(i)
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a {what}"))
            };
            match name {
                "-h" | "--help" => return Err(HELP.to_string()),
                "--screen" => {
                    let v = value("name")?;
                    args.screen = match v.as_str() {
                        "this-pc" => Screen::ThisPc,
                        "quick-access" => Screen::QuickAccess,
                        "home" => Screen::Home,
                        "settings" => Screen::Settings,
                        other => {
                            return Err(format!(
                                "--screen: expected this-pc | quick-access | home | settings, \
                                 got {other:?}"
                            ))
                        }
                    };
                }
                "--theme" => {
                    let v = value("name")?;
                    match v.as_str() {
                        "flat" | "flora" => args.theme = Some(v),
                        other => {
                            return Err(format!("--theme: expected flat | flora, got {other:?}"))
                        }
                    }
                }
                "--mode" => {
                    let v = value("mode")?;
                    args.dark = match v.as_str() {
                        "dark" => Some(true),
                        "light" => Some(false),
                        other => {
                            return Err(format!("--mode: expected light | dark, got {other:?}"))
                        }
                    };
                }
                "--layout" => {
                    let v = value("name")?;
                    args.layout = Some(ViewLayout::from_name(&v).ok_or_else(|| {
                        format!(
                            "--layout: expected large-icons, list, details, tiles, ... got {v:?}"
                        )
                    })?);
                }
                "--sample" => args.sample = true,
                other => return Err(format!("unknown option {other:?}\n\n{HELP}")),
            }
            i += 1;
        }
        Ok(args)
    }
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
/// (never over a file that is there). Returns how many it wrote.
pub fn write_sample(home: &Path) -> Result<usize, String> {
    let drive = LocalDrive::new(home);
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

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().map(|s| s.to_string()))
    }

    #[test]
    fn no_switches_start_where_the_settings_say() {
        let args = parse(&[]).unwrap();
        assert_eq!(args.screen, Screen::Default);
        assert_eq!(args.theme, None);
        assert_eq!(args.dark, None);
        assert_eq!(args.layout, None);
        assert!(!args.sample);
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
        ])
        .unwrap();
        assert_eq!(args.screen, Screen::Home);
        assert_eq!(args.theme.as_deref(), Some("flora"));
        assert_eq!(args.dark, Some(true));
        assert_eq!(args.layout, Some(ViewLayout::LargeIcons));
        assert!(args.sample);
        assert_eq!(parse(&["--mode", "light"]).unwrap().dark, Some(false));
        assert_eq!(
            parse(&["--screen", "quick-access"]).unwrap().screen,
            Screen::QuickAccess
        );
        assert_eq!(
            parse(&["--screen=settings"]).unwrap().screen,
            Screen::Settings
        );
    }

    #[test]
    fn a_wrong_switch_or_value_says_what_is_expected() {
        assert!(parse(&["--screen", "moon"]).unwrap_err().contains("this-pc"));
        assert!(parse(&["--theme", "neon"]).unwrap_err().contains("flora"));
        assert!(parse(&["--mode"]).unwrap_err().contains("--mode"));
        assert!(parse(&["--layout", "huge"]).unwrap_err().contains("details"));
        assert!(parse(&["--what"]).unwrap_err().contains("unknown"));
        assert!(parse(&["--help"]).unwrap_err().contains("USAGE"));
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
