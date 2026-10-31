//! The command line, on azul-appkit's switches (`--screen`, `--size`,
//! `--theme`, `--mode`, `--shot`, `--sample`, `--data-dir`) plus a bare
//! project id: `AzVideoCut [OPTIONS] [PROJECT]`.

use azul_appkit::args::{AppArgs, AppSpec};

/// The screen the window opens on.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    /// The editor (or the empty state without a project).
    #[default]
    Editor,
    /// The editor with the export dialog open.
    Export,
    /// The settings.
    Settings,
    /// The about page.
    About,
}

/// The names `--screen` takes; the first is the default.
pub const SCREENS: [&str; 4] = ["editor", "export", "settings", "about"];

/// What the kit's parser and usage text know about AzVideoCut.
pub const SPEC: AppSpec = AppSpec {
    name: "AzVideoCut",
    binary: "AzVideoCut",
    summary: "a video editor on azul's video stack",
    screens: &SCREENS,
    files_help: "a project id to open (videocut/<id>/project.json in the data folder)",
};

/// AzVideoCut's reading of the kit's switches.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// Make the sample project (generated clips; encoded to MP4 where the
    /// machine has an H.264 encoder) and open it.
    pub sample: bool,
    /// Open this project (a bare argument).
    pub project: Option<String>,
    pub screen: Screen,
}

pub type ParseError = String;

impl Args {
    /// AzVideoCut's switches from the kit's.
    pub fn from_app(a: &AppArgs) -> Result<Self, ParseError> {
        if a.files.len() > 1 {
            return Err(format!("more than one project given ({:?})", a.files));
        }
        let screen = match a.screen_or_default(&SPEC) {
            "export" => Screen::Export,
            "settings" => Screen::Settings,
            "about" => Screen::About,
            _ => Screen::Editor,
        };
        Ok(Self {
            sample: a.sample,
            project: a.files.first().map(|p| p.to_string_lossy().into_owned()),
            screen,
        })
    }

    /// The arguments after the program name, through the kit's parser.
    pub fn parse<I, S>(argv: I) -> Result<(AppArgs, Self), ParseError>
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
#[path = "args_tests.rs"]
mod tests;
