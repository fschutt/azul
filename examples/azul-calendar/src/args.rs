//! AzCalendar's command line, as AzWriter's: `--screen <name>` opens a view, a FILE (backstage)
//! page or the event editor (for scripts and screenshots), `--theme flat|flora` and
//! `--mode light|dark` pick the app theme and the mode, `--sample` puts sample events into an
//! empty calendar, `--date YYYY-MM-DD` opens on that day, `--data <dir>` is the data folder
//! (as `AZCAL_DATA`), `--worker <url>` the meeting server for this run (AzMeet's switch).

use std::path::PathBuf;

use chrono::NaiveDate;

use crate::views::ViewKind;

/// The FILE (backstage) pages, in the order of its list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BackstagePage {
    Info,
    /// Open & Export: import an .ics file, export a calendar as one.
    Open,
    Print,
    /// Manage calendars: add, rename, recolour, remove.
    Calendars,
    /// Options: the meeting server, the look.
    Options,
    About,
}

impl BackstagePage {
    pub const ALL: [BackstagePage; 6] = [
        BackstagePage::Info,
        BackstagePage::Open,
        BackstagePage::Print,
        BackstagePage::Calendars,
        BackstagePage::Options,
        BackstagePage::About,
    ];

    /// What the page list says (a key of the resources).
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            BackstagePage::Info => "azcalendar-file-info",
            BackstagePage::Open => "azcalendar-file-open",
            BackstagePage::Print => "azcalendar-file-print",
            BackstagePage::Calendars => "azcalendar-file-calendars",
            BackstagePage::Options => "azcalendar-file-options",
            BackstagePage::About => "azcalendar-file-about",
        }
    }

    /// The page's name after `backstage-` in `--screen`.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            BackstagePage::Info => "info",
            BackstagePage::Open => "open",
            BackstagePage::Print => "print",
            BackstagePage::Calendars => "calendars",
            BackstagePage::Options => "options",
            BackstagePage::About => "about",
        }
    }

    /// The page at `index` of the list.
    #[must_use]
    pub fn at(index: usize) -> Option<BackstagePage> {
        BackstagePage::ALL.get(index).copied()
    }

    /// The page's index in the list.
    #[must_use]
    pub fn index(self) -> usize {
        BackstagePage::ALL
            .iter()
            .position(|p| *p == self)
            .unwrap_or(0)
    }
}

/// What the window opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    View(ViewKind),
    Backstage(BackstagePage),
    /// The event editor window, with a new appointment.
    Editor,
}

/// Light or dark, when the command line says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub screen: Option<Screen>,
    /// "flat" or "flora".
    pub theme: Option<String>,
    pub mode: Option<Mode>,
    pub sample: bool,
    pub date: Option<NaiveDate>,
    pub data: Option<PathBuf>,
    /// `--worker <url>`: the meeting server for this run (AzMeet's switch).
    pub worker: Option<String>,
    /// `--language system|en|de`: the language of the words for this run (else Options').
    pub language: Option<azul_appkit::args::LanguagePref>,
}

pub const HELP: &str = "\
AzCalendar - a calendar like Outlook's, with AzMeet links

USAGE:
    AzCalendar [OPTIONS]

OPTIONS:
    --screen <NAME>     day | work-week | week | month | schedule | agenda | editor |
                        backstage-info | backstage-open | backstage-print |
                        backstage-calendars | backstage-options | backstage-about
    --theme <NAME>      flat | flora (the app theme)
    --mode <MODE>       light | dark (else the system's)
    --sample            put sample events into an empty calendar
    --date <DATE>       open on this day (YYYY-MM-DD)
    --data <DIR>        the data folder (else AZCAL_DATA, else the user's data folder)
    --worker <URL>      the meeting server for this run (else the saved one, AZMEET_WORKER,
                        endpoints.meet of the shared Azlin config, the built-in one)
    --language <LANG>   system | en | de (else Options', else the system's)
    -h, --help          print this help
";

/// The screen `--screen` names.
fn screen_of(name: &str) -> Option<Screen> {
    if name == "editor" {
        return Some(Screen::Editor);
    }
    if let Some(page) = name.strip_prefix("backstage-") {
        return BackstagePage::ALL
            .into_iter()
            .find(|p| p.name() == page)
            .map(Screen::Backstage);
    }
    ViewKind::from_name(name).map(Screen::View)
}

impl Args {
    /// Reads the arguments after the program's name. `Err` holds what to print: the help, or
    /// what is wrong and the help.
    pub fn parse<I, S>(argv: I) -> Result<Args, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut a = Args::default();
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
                    .ok_or_else(|| format!("{name} needs a {what}\n\n{HELP}"))
            };
            match name {
                "-h" | "--help" => return Err(HELP.to_string()),
                "--screen" => {
                    let v = value("name")?;
                    a.screen = Some(
                        screen_of(&v)
                            .ok_or_else(|| format!("--screen: unknown {v:?}\n\n{HELP}"))?,
                    );
                }
                "--theme" => {
                    let v = value("theme")?;
                    if !matches!(v.as_str(), "flat" | "flora") {
                        return Err(format!("--theme: expected flat or flora, got {v:?}"));
                    }
                    a.theme = Some(v);
                }
                "--mode" => {
                    let v = value("mode")?;
                    a.mode = Some(match v.as_str() {
                        "light" => Mode::Light,
                        "dark" => Mode::Dark,
                        other => {
                            return Err(format!("--mode: expected light or dark, got {other:?}"))
                        }
                    });
                }
                "--sample" => a.sample = true,
                "--date" => {
                    let v = value("date")?;
                    a.date = Some(
                        NaiveDate::parse_from_str(&v, "%Y-%m-%d")
                            .map_err(|_| format!("--date: expected YYYY-MM-DD, got {v:?}"))?,
                    );
                }
                "--data" => a.data = Some(PathBuf::from(value("folder")?)),
                "--worker" => a.worker = Some(value("meeting server")?),
                "--language" => {
                    let v = value("language")?;
                    a.language = Some(azul_appkit::args::LanguagePref::parse(&v).ok_or_else(|| {
                        format!("--language: expected system, en or de, got {v:?}")
                    })?);
                }
                other => return Err(format!("unknown option {other:?}\n\n{HELP}")),
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
    fn no_arguments_open_the_calendar_as_it_was() {
        assert_eq!(parse(&[]), Ok(Args::default()));
    }

    #[test]
    fn a_screen_names_a_view_a_backstage_page_or_the_editor() {
        assert_eq!(
            parse(&["--screen", "month"]).unwrap().screen,
            Some(Screen::View(ViewKind::Month))
        );
        assert_eq!(
            parse(&["--screen=work-week"]).unwrap().screen,
            Some(Screen::View(ViewKind::WorkWeek))
        );
        assert_eq!(
            parse(&["--screen", "backstage-open"]).unwrap().screen,
            Some(Screen::Backstage(BackstagePage::Open))
        );
        assert_eq!(
            parse(&["--screen", "editor"]).unwrap().screen,
            Some(Screen::Editor)
        );
        assert!(parse(&["--screen", "backstage-nothing"]).is_err());
        assert!(parse(&["--screen"]).is_err());
    }

    #[test]
    fn the_theme_the_mode_the_day_the_sample_and_the_data_folder() {
        let a = parse(&[
            "--theme",
            "flora",
            "--mode",
            "dark",
            "--date",
            "2026-09-30",
            "--sample",
            "--data",
            "/tmp/cal",
        ])
        .unwrap();
        assert_eq!(a.theme.as_deref(), Some("flora"));
        assert_eq!(a.mode, Some(Mode::Dark));
        assert_eq!(a.date, NaiveDate::from_ymd_opt(2026, 9, 30));
        assert!(a.sample);
        assert_eq!(a.data, Some(PathBuf::from("/tmp/cal")));
        assert!(parse(&["--theme", "neon"]).is_err());
        assert!(parse(&["--mode", "dim"]).is_err());
        assert!(parse(&["--date", "30.09.2026"]).is_err());
        assert!(parse(&["--frobnicate"]).is_err());
        assert_eq!(parse(&["--help"]), Err(HELP.to_string()));
    }

    #[test]
    fn every_backstage_page_has_a_name_that_reads_back() {
        for page in BackstagePage::ALL {
            assert_eq!(
                screen_of(&format!("backstage-{}", page.name())),
                Some(Screen::Backstage(page))
            );
            assert_eq!(BackstagePage::at(page.index()), Some(page));
        }
    }

    #[test]
    fn the_worker_switch_names_the_meeting_server_for_this_run() {
        assert_eq!(
            parse(&["--worker", "http://127.0.0.1:8790"])
                .unwrap()
                .worker
                .as_deref(),
            Some("http://127.0.0.1:8790")
        );
        assert_eq!(
            parse(&["--worker=https://meet.example.com"])
                .unwrap()
                .worker
                .as_deref(),
            Some("https://meet.example.com")
        );
        assert!(parse(&["--worker"]).is_err());
    }
}
