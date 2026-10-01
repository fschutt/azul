//! AzSetup's model without azul types: the steps, the command line, the
//! component table and the simulated copying. Tested without a window.

/// One megabyte.
pub const MB: u64 = 1024 * 1024;

/// The wizard's steps, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Step {
    Welcome,
    License,
    Destination,
    Components,
    Options,
    Ready,
    Installing,
    Finish,
}

/// Every step, in order.
pub const STEPS: [Step; 8] = [
    Step::Welcome,
    Step::License,
    Step::Destination,
    Step::Components,
    Step::Options,
    Step::Ready,
    Step::Installing,
    Step::Finish,
];

impl Step {
    /// The step's label on the rail / side panel.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Welcome => "Welcome",
            Self::License => "License",
            Self::Destination => "Destination",
            Self::Components => "Components",
            Self::Options => "Options",
            Self::Ready => "Ready to install",
            Self::Installing => "Installing",
            Self::Finish => "Finish",
        }
    }

    /// The banner's subtitle for the step.
    #[must_use]
    pub const fn subtitle(self) -> &'static str {
        match self {
            Self::Welcome => "",
            Self::License => "Please review the license terms before installing AzOffice.",
            Self::Destination => "Where should AzOffice be installed?",
            Self::Components => "Which parts of AzOffice should be installed?",
            Self::Options => "Which additional tasks should Setup perform?",
            Self::Ready => "Setup is ready to install AzOffice on your computer.",
            Self::Installing => "Please wait while Setup installs AzOffice.",
            Self::Finish => "",
        }
    }

    /// The step's index.
    #[must_use]
    pub fn index(self) -> usize {
        STEPS.iter().position(|s| *s == self).unwrap_or(0)
    }

    /// The step at `index` (the last one past the end).
    #[must_use]
    pub fn at(index: usize) -> Self {
        STEPS[index.min(STEPS.len() - 1)]
    }

    /// The step after this one (the last stays).
    #[must_use]
    pub fn next(self) -> Self {
        Self::at(self.index() + 1)
    }

    /// The step before this one (the first stays). Nothing goes back from
    /// the copying or the finish page.
    #[must_use]
    pub fn back(self) -> Self {
        match self {
            Self::Installing | Self::Finish | Self::Welcome => self,
            _ => Self::at(self.index() - 1),
        }
    }

    /// Whether Back does anything on this step.
    #[must_use]
    pub const fn can_go_back(self) -> bool {
        !matches!(self, Self::Welcome | Self::Installing | Self::Finish)
    }

    /// Whether the welcome / finish frame (the side panel) shows the step,
    /// Wizard97's way; the others take the banner.
    #[must_use]
    pub const fn is_outer(self) -> bool {
        matches!(self, Self::Welcome | Self::Finish)
    }
}

/// One component of the fake AzOffice: `(label, size in MB, depth,
/// ticked, required, description)`.
pub type ComponentRow = (&'static str, u64, u32, bool, bool, &'static str);

/// The components AzSetup offers, in tree order.
pub const COMPONENTS: &[ComponentRow] = &[
    (
        "AzOffice core",
        180,
        0,
        true,
        true,
        "The shared libraries every program needs.",
    ),
    ("Applications", 0, 0, true, false, ""),
    ("AzWriter", 120, 1, true, false, "Letters, reports, books."),
    ("AzSheets", 90, 1, true, false, "Tables, charts, budgets."),
    ("AzSlides", 70, 1, true, false, "Presentations."),
    (
        "Templates and clip art",
        45,
        0,
        true,
        false,
        "Invoices, CVs, calendars.",
    ),
    ("Proofing tools", 30, 0, true, false, ""),
    ("English", 12, 1, true, false, ""),
    ("Deutsch", 12, 1, false, false, ""),
];

/// The default total of the ticked components, in bytes.
#[must_use]
pub fn default_total() -> u64 {
    COMPONENTS.iter().filter(|c| c.3).map(|c| c.1 * MB).sum()
}

/// The license AzSetup shows (paragraphs `\n\n` apart).
pub const LICENSE: &str = "AZOFFICE LICENSE AGREEMENT\n\n\
Permission is hereby granted, free of charge, to any person obtaining a copy of this \
software and associated documentation files (the \"Software\"), to deal in the Software \
without restriction, including without limitation the rights to use, copy, modify, merge, \
publish, distribute, sublicense, and/or sell copies of the Software.\n\n\
The above copyright notice and this permission notice shall be included in all copies or \
substantial portions of the Software.\n\n\
THE SOFTWARE IS PROVIDED \"AS IS\", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, \
INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR \
PURPOSE AND NONINFRINGEMENT.\n\n\
This is a demonstration: AzSetup installs nothing.";

// ---------------------------------------------------------------------------
// The simulated copying
// ---------------------------------------------------------------------------

/// The copying the progress page shows: a queue of fake files, copied a
/// few per timer tick.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Copier {
    /// The files still to copy: `(path, bytes)`, in order.
    pub queue: Vec<(String, u64)>,
    /// The bytes copied so far.
    pub done_bytes: u64,
    /// The bytes of every file.
    pub total_bytes: u64,
    /// What was done, oldest first.
    pub log: Vec<String>,
    /// The file copied last.
    pub current: String,
}

impl Copier {
    /// The copying of `components` (`(label, bytes)`) into `folder`: one
    /// fake file per started 10 MB of each component.
    #[must_use]
    pub fn plan(folder: &str, components: &[(String, u64)]) -> Self {
        let mut queue = Vec::new();
        for (label, bytes) in components {
            if *bytes == 0 {
                continue;
            }
            let slug: String = label
                .chars()
                .map(|c| {
                    if c.is_ascii_alphanumeric() {
                        c.to_ascii_lowercase()
                    } else {
                        '-'
                    }
                })
                .collect();
            let files = bytes.div_ceil(10 * MB).max(1);
            let each = bytes / files;
            for i in 0..files {
                let size = if i + 1 == files {
                    bytes - each * (files - 1)
                } else {
                    each
                };
                queue.push((format!("{folder}/{slug}/part-{:02}.bin", i + 1), size));
            }
        }
        let total_bytes = queue.iter().map(|(_, b)| b).sum();
        Self {
            queue,
            done_bytes: 0,
            total_bytes,
            log: vec![format!("Created {folder}")],
            current: String::new(),
        }
    }

    /// Copies up to `files` more files; `true` once everything is copied.
    pub fn tick(&mut self, files: usize) -> bool {
        for _ in 0..files {
            if self.queue.is_empty() {
                break;
            }
            let (path, bytes) = self.queue.remove(0);
            self.done_bytes += bytes;
            self.log.push(format!("Copied {path}"));
            self.current = path;
        }
        self.is_done()
    }

    /// Whether everything is copied.
    #[must_use]
    pub fn is_done(&self) -> bool {
        self.queue.is_empty()
    }

    /// How far along, 0 to 100.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn percent(&self) -> f32 {
        if self.total_bytes == 0 {
            return if self.is_done() { 100.0 } else { 0.0 };
        }
        (self.done_bytes as f64 * 100.0 / self.total_bytes as f64) as f32
    }
}

// ---------------------------------------------------------------------------
// The command line
// ---------------------------------------------------------------------------

/// What the window opens on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Screen {
    /// The install wizard.
    #[default]
    Setup,
    /// The settings window.
    Settings,
    /// The wizard with the About box open.
    About,
}

/// How the wizard frames its pages.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Frame {
    /// Wizard97: the side panel on welcome and finish, the banner between.
    #[default]
    Installer,
    /// The steps rail everywhere.
    Rail,
    /// The macOS installer's side panel everywhere.
    Side,
}

/// The command line.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Args {
    pub screen: Screen,
    pub frame: Frame,
    /// `flat` or `flora`, or none (the default theme).
    pub theme: Option<String>,
    /// `light` or `dark`, or none (the system's).
    pub mode: Option<String>,
    /// The step to open on (`--step 3`).
    pub step: usize,
}

pub const HELP: &str = "\
AzSetup - a fake installer for AzOffice (installs nothing)

USAGE:
    AzSetup [OPTIONS]

OPTIONS:
    --screen <NAME>    setup | settings | about
    --frame <NAME>     installer | rail | side
    --theme <NAME>     flat | flora
    --mode <NAME>      light | dark
    --step <N>         open the wizard on step N (0 = welcome)
    -h, --help         print this help
";

impl Args {
    /// Parses `argv` (without the program name).
    ///
    /// # Errors
    /// The reason, for an unknown option or a bad value.
    pub fn parse<I, S>(argv: I) -> Result<Self, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut args = Self::default();
        let mut i = 0;
        while i < argv.len() {
            let (name, inline) = match argv[i].split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n.to_string(), Some(v.to_string())),
                _ => (argv[i].clone(), None),
            };
            let mut value = || -> Result<String, String> {
                if let Some(v) = inline.clone() {
                    return Ok(v);
                }
                i += 1;
                argv.get(i)
                    .cloned()
                    .ok_or_else(|| format!("{name} needs a value"))
            };
            match name.as_str() {
                "--screen" => {
                    args.screen = match value()?.as_str() {
                        "setup" => Screen::Setup,
                        "settings" => Screen::Settings,
                        "about" => Screen::About,
                        other => return Err(format!("unknown screen {other:?}")),
                    }
                }
                "--frame" => {
                    args.frame = match value()?.as_str() {
                        "installer" => Frame::Installer,
                        "rail" => Frame::Rail,
                        "side" => Frame::Side,
                        other => return Err(format!("unknown frame {other:?}")),
                    }
                }
                "--theme" => {
                    let t = value()?;
                    if t != "flat" && t != "flora" {
                        return Err(format!("unknown theme {t:?}"));
                    }
                    args.theme = Some(t);
                }
                "--mode" => {
                    let m = value()?;
                    if m != "light" && m != "dark" {
                        return Err(format!("unknown mode {m:?}"));
                    }
                    args.mode = Some(m);
                }
                "--step" => {
                    args.step = value()?
                        .parse()
                        .map_err(|_| "--step needs a number".to_string())?;
                }
                "-h" | "--help" => return Err(HELP.to_string()),
                other => return Err(format!("unknown option {other:?}\n\n{HELP}")),
            }
            i += 1;
        }
        Ok(args)
    }
}

// ---------------------------------------------------------------------------
// stdout, for the scripts
// ---------------------------------------------------------------------------

/// `AZSETUP_STEP <index> <label>`.
#[must_use]
pub fn step_line(step: Step) -> String {
    format!("AZSETUP_STEP {} {}", step.index(), step.label())
}

/// `AZSETUP_TOTAL <bytes>`.
#[must_use]
pub fn total_line(bytes: u64) -> String {
    format!("AZSETUP_TOTAL {bytes}")
}

/// `AZSETUP_PROGRESS <percent>` (rounded).
#[must_use]
#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
pub fn progress_line(percent: f32) -> String {
    format!(
        "AZSETUP_PROGRESS {}",
        percent.clamp(0.0, 100.0).round() as u32
    )
}

#[cfg(test)]
mod model_tests {
    use super::*;

    #[test]
    fn the_steps_go_forward_and_back_except_from_the_copying_and_the_end() {
        assert_eq!(Step::Welcome.next(), Step::License);
        assert_eq!(Step::Ready.next(), Step::Installing);
        assert_eq!(Step::Finish.next(), Step::Finish);
        assert_eq!(Step::Components.back(), Step::Destination);
        assert_eq!(Step::Welcome.back(), Step::Welcome);
        assert_eq!(Step::Installing.back(), Step::Installing);
        assert!(!Step::Finish.can_go_back() && !Step::Installing.can_go_back());
        assert!(Step::License.can_go_back());
        assert!(Step::Welcome.is_outer() && Step::Finish.is_outer() && !Step::License.is_outer());
        assert_eq!(Step::at(99), Step::Finish);
        assert_eq!(Step::Options.index(), 4);
    }

    #[test]
    fn the_default_components_add_up() {
        // 180 + 120 + 90 + 70 + 45 + 30 + 12 MB: Deutsch is not ticked.
        assert_eq!(default_total(), 547 * MB);
        assert!(COMPONENTS[0].4, "the core is required");
    }

    #[test]
    fn the_copier_copies_every_byte_and_reaches_a_hundred_percent() {
        let mut c = Copier::plan(
            "/opt/AzOffice",
            &[
                ("AzWriter".to_string(), 25 * MB),
                ("Groups".to_string(), 0),
                ("AzSlides".to_string(), 5 * MB),
            ],
        );
        assert_eq!(c.total_bytes, 30 * MB);
        assert_eq!(
            c.queue.len(),
            4,
            "3 parts of AzWriter, 1 of AzSlides; a group copies nothing"
        );
        assert_eq!(c.queue[0].0, "/opt/AzOffice/azwriter/part-01.bin");
        assert_eq!(c.percent(), 0.0);
        assert!(!c.tick(2));
        assert!(c.percent() > 0.0 && c.percent() < 100.0);
        assert!(c.tick(10));
        assert_eq!(c.percent(), 100.0);
        assert_eq!(c.done_bytes, 30 * MB);
        assert_eq!(c.current, "/opt/AzOffice/azslides/part-01.bin");
        assert_eq!(c.log.len(), 5, "the folder, then four files");
        assert!(
            Copier::plan("/x", &[]).tick(1),
            "nothing to copy is done at once"
        );
    }

    #[test]
    fn the_command_line_picks_the_screen_the_frame_the_theme_and_the_mode() {
        let a = Args::parse([
            "--screen",
            "settings",
            "--theme=flora",
            "--mode",
            "dark",
            "--frame",
            "side",
            "--step",
            "3",
        ])
        .expect("valid");
        assert_eq!(a.screen, Screen::Settings);
        assert_eq!(a.frame, Frame::Side);
        assert_eq!(a.theme.as_deref(), Some("flora"));
        assert_eq!(a.mode.as_deref(), Some("dark"));
        assert_eq!(a.step, 3);
        assert_eq!(
            Args::parse(Vec::<String>::new()).expect("empty"),
            Args::default()
        );
        assert!(Args::parse(["--theme", "neon"]).is_err());
        assert!(Args::parse(["--bogus"]).is_err());
    }

    #[test]
    fn the_stdout_lines_name_the_step_the_total_and_the_progress() {
        assert_eq!(step_line(Step::Destination), "AZSETUP_STEP 2 Destination");
        assert_eq!(total_line(5), "AZSETUP_TOTAL 5");
        assert_eq!(progress_line(99.6), "AZSETUP_PROGRESS 100");
        assert_eq!(progress_line(-3.0), "AZSETUP_PROGRESS 0");
    }
}
