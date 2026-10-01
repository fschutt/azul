//! Command line: `AzPhoto [--sample] [--open FILE] [--screen NAME]
//! [--theme flat|flora] [--mode light|dark] [--size WxH] [--data DIR]
//! [--export-dir DIR]`.

use std::path::PathBuf;

/// The screen the window opens on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Screen {
    /// The start screen without a document; the editor with one.
    #[default]
    Auto,
    Start,
    Editor,
    /// The editor with the export sheet open.
    Export,
    /// The editor with the new-image sheet open.
    NewImage,
    Settings,
    About,
}

impl Screen {
    pub const NAMES: [&'static str; 7] = ["auto", "start", "editor", "export", "new", "settings", "about"];

    #[must_use]
    pub fn from_name(name: &str) -> Option<Self> {
        Some(match name {
            "auto" => Self::Auto,
            "start" => Self::Start,
            "editor" => Self::Editor,
            "export" => Self::Export,
            "new" => Self::NewImage,
            "settings" => Self::Settings,
            "about" => Self::About,
            _ => return None,
        })
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// Open the sample: a generated photo, a second layer and an adjustment.
    pub sample: bool,
    /// An image (PNG / JPEG / WebP / ...) to open.
    pub open: Option<PathBuf>,
    pub screen: Screen,
    /// `flat` or `flora`; `None` = azul's default.
    pub theme: Option<String>,
    /// `Some(true)` = dark, `Some(false)` = light, `None` = follow the OS.
    pub dark: Option<bool>,
    pub size: Option<(f32, f32)>,
    /// The data root (the local stand-in for the user's bucket).
    pub data: Option<PathBuf>,
    /// Export straight into this folder instead of asking (scripts).
    pub export_dir: Option<PathBuf>,
}

pub const HELP: &str = "\
AzPhoto - a photo editor

USAGE:
    AzPhoto [OPTIONS] [IMAGE]

OPTIONS:
    --sample               Open the sample photo (two layers and an adjustment)
    --open <IMAGE>         An image to open (PNG, JPEG, WebP, ...)
    --screen <NAME>        auto | start | editor | export | new | settings | about
    --theme <NAME>         flat | flora
    --mode <MODE>          light | dark
    --size <WxH>           The window size, e.g. 1400x900
    --data <DIR>           The data folder (default: AZPHOTO_DATA, else <user data>/Azul)
    --export-dir <DIR>     Export there without a dialog (default: AZPHOTO_EXPORT_DIR)
    -h, --help             This text
";

impl Args {
    /// Parse the arguments after the program name.
    pub fn parse<I, S>(argv: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut a = Self::default();
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
                argv.get(i).cloned().ok_or_else(|| format!("{name} needs a {what}"))
            };
            match name {
                "-h" | "--help" => return Err(HELP.to_string()),
                "--sample" => a.sample = true,
                "--open" => a.open = Some(PathBuf::from(value("file")?)),
                "--screen" => {
                    let v = value("name")?;
                    a.screen = Screen::from_name(&v).ok_or_else(|| {
                        format!("--screen: expected {}, got {v:?}", Screen::NAMES.join("|"))
                    })?;
                }
                "--theme" => {
                    let v = value("name")?;
                    if v != "flat" && v != "flora" {
                        return Err(format!("--theme: expected flat|flora, got {v:?}"));
                    }
                    a.theme = Some(v);
                }
                "--mode" => {
                    let v = value("mode")?;
                    a.dark = Some(match v.as_str() {
                        "light" => false,
                        "dark" => true,
                        other => return Err(format!("--mode: expected light|dark, got {other:?}")),
                    });
                }
                "--size" => {
                    let v = value("WxH")?;
                    let parsed = v
                        .split_once('x')
                        .and_then(|(w, h)| Some((w.parse::<f32>().ok()?, h.parse::<f32>().ok()?)))
                        .filter(|(w, h)| *w > 0.0 && *h > 0.0);
                    a.size = Some(parsed.ok_or_else(|| format!("--size: expected WxH, got {v:?}"))?);
                }
                "--data" => a.data = Some(PathBuf::from(value("folder")?)),
                "--export-dir" => a.export_dir = Some(PathBuf::from(value("folder")?)),
                other if other.starts_with('-') => return Err(format!("unknown option {other:?}\n\n{HELP}")),
                positional => {
                    if a.open.is_some() {
                        return Err(format!("more than one image given ({positional:?})"));
                    }
                    a.open = Some(PathBuf::from(positional));
                }
            }
            i += 1;
        }
        Ok(a)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Args, String> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_open_the_start_screen_in_the_default_look() {
        let a = parse(&[]).unwrap();
        assert_eq!(a, Args::default());
        assert_eq!(a.screen, Screen::Auto);
    }

    #[test]
    fn the_switches_of_every_azul_app_are_understood() {
        let a = parse(&["--sample", "--screen", "export", "--theme=flora", "--mode", "dark", "--size", "1400x900"]).unwrap();
        assert!(a.sample);
        assert_eq!(a.screen, Screen::Export);
        assert_eq!(a.theme.as_deref(), Some("flora"));
        assert_eq!(a.dark, Some(true));
        assert_eq!(a.size, Some((1400.0, 900.0)));
    }

    #[test]
    fn a_bare_path_is_the_image_to_open() {
        assert_eq!(parse(&["shot.png"]).unwrap().open, Some(PathBuf::from("shot.png")));
        assert!(parse(&["a.png", "b.png"]).is_err());
    }

    #[test]
    fn a_bad_value_is_an_error_not_a_default() {
        for bad in [&["--theme", "neon"][..], &["--mode", "dim"], &["--screen", "x"], &["--size", "big"], &["--nope"], &["--open"]] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert!(parse(&["--help"]).unwrap_err().contains("USAGE"));
    }
}
