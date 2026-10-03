//! Command line, on azul-appkit's switches (`--screen`, `--size`, `--theme`,
//! `--mode`, `--shot`, `--sample`, `--data-dir`) - `AzPhoto [OPTIONS]
//! [IMAGE]`. AzPhoto adds only what the switches MEAN for it: the screen to
//! open and the image to open (a bare argument).

use std::path::PathBuf;

use azul_appkit::args::{AppArgs, AppSpec};

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
    /// The names `--screen` takes; the first is the default.
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

/// What the kit's parser and usage text know about AzPhoto.
pub const SPEC: AppSpec = AppSpec {
    name: "AzPhoto",
    binary: "AzPhoto",
    summary: "a photo editor: layers, brushes, selections, adjustments, filters, text",
    screens: &Screen::NAMES,
    files_help: "an image to open (PNG, JPEG, WebP, GIF, BMP, TIFF, TGA)",
};

/// AzPhoto's reading of the switches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// Open the sample: the photo, a light leak layer and an adjustment.
    pub sample: bool,
    /// An image (PNG / JPEG / WebP / ...) to open.
    pub open: Option<PathBuf>,
    pub screen: Screen,
}

impl Args {
    /// AzPhoto's switches from the kit's.
    pub fn from_app(a: &AppArgs) -> Result<Self, String> {
        if a.files.len() > 1 {
            return Err(format!("more than one image given ({:?})", a.files));
        }
        Ok(Self {
            sample: a.sample,
            open: a.files.first().cloned(),
            screen: Screen::from_name(a.screen_or_default(&SPEC)).unwrap_or_default(),
        })
    }

    /// Parse the arguments after the program name (the kit's parser).
    pub fn parse<I, S>(argv: I) -> Result<(AppArgs, Self), String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let app = AppArgs::parse(&SPEC, argv)?;
        let args = Self::from_app(&app)?;
        Ok((app, args))
    }
}

#[cfg(test)]
mod tests {
    use azul_appkit::args::{ModePref, Theme};

    use super::*;

    fn parse(args: &[&str]) -> Result<(AppArgs, Args), String> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_open_the_start_screen_in_the_default_look() {
        let (app, a) = parse(&[]).unwrap();
        assert_eq!(a, Args::default());
        assert_eq!(a.screen, Screen::Auto);
        assert_eq!((app.theme, app.mode), (None, None), "the settings file decides");
    }

    #[test]
    fn the_switches_of_every_azul_app_are_understood() {
        let (app, a) =
            parse(&["--sample", "--screen", "export", "--theme=flora", "--mode", "dark", "--size", "1400x900"]).unwrap();
        assert!(a.sample);
        assert_eq!(a.screen, Screen::Export);
        assert_eq!(app.theme, Some(Theme::Flora));
        assert_eq!(app.mode, Some(ModePref::Dark));
        assert_eq!(app.size, Some((1400.0, 900.0)));
        let (app, _) = parse(&["--data-dir", "/tmp/az", "--mode", "system"]).unwrap();
        assert_eq!(app.data_dir, Some(PathBuf::from("/tmp/az")));
        assert_eq!(app.mode, Some(ModePref::System));
    }

    #[test]
    fn a_bare_path_is_the_image_to_open() {
        assert_eq!(parse(&["shot.png"]).unwrap().1.open, Some(PathBuf::from("shot.png")));
        assert!(parse(&["a.png", "b.png"]).is_err());
    }

    #[test]
    fn a_bad_value_is_an_error_not_a_default() {
        for bad in [&["--theme", "neon"][..], &["--mode", "dim"], &["--screen", "x"], &["--size", "big"], &["--nope"]] {
            assert!(parse(bad).is_err(), "{bad:?}");
        }
        assert!(parse(&["--help"]).unwrap_err().contains("USAGE"));
    }
}
