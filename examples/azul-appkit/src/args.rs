//! The command-line switches every Azlin app understands (build ledger F2).
//!
//! AzWriter's `args.rs` is the model: `--screen <name>` opens a screen
//! directly, `--size <WxH>` sizes the window, `--theme <flat|flora|flora:green|...>` and
//! `--mode <light|dark|system>` pick the app theme and the light / dark mode
//! (they win over the settings file for this run and are not saved),
//! `--shot <PNG>` renders, writes a screenshot and exits (the screenshot
//! regression of F8), `--sample` fills an empty data folder with sample data,
//! `--data-dir <DIR>` points the app at another data root. Bare arguments
//! are files for the app (AzContacts imports `.vcf` files given this way).
//!
//! No azul types here: the parser is tested without a window.

use std::path::PathBuf;

/// The app theme: what a switch, the shared config, the settings file and
/// the settings page name. `flat`, `flora` - the website's look, its deep
/// blue stone - and flora's SPINS: the same look cut in another accent stone,
/// the liturgical set of the design system (`flora:green` Ordinary,
/// `flora:red` Pentecost, `flora:purple` Advent, `flora:gold` Easter,
/// `flora:rose` Gaudete; azul's `widgets::themes::spin`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum Theme {
    #[default]
    Flat,
    Flora,
    FloraGreen,
    FloraRed,
    FloraPurple,
    FloraGold,
    FloraRose,
}

impl Theme {
    /// Every theme, in the order the settings page lists them.
    pub const ALL: [Theme; 7] = [
        Theme::Flat,
        Theme::Flora,
        Theme::FloraGreen,
        Theme::FloraRed,
        Theme::FloraPurple,
        Theme::FloraGold,
        Theme::FloraRose,
    ];

    /// Flora's stones, the base (blue) first: the settings page's stone
    /// picker.
    pub const FLORA: [Theme; 6] = [
        Theme::Flora,
        Theme::FloraGreen,
        Theme::FloraRed,
        Theme::FloraPurple,
        Theme::FloraGold,
        Theme::FloraRose,
    ];

    /// The name azul knows the theme by (`AppConfig::with_theme`,
    /// `CallbackInfo::set_theme`, `AZ_THEME`): `flat`, `flora`,
    /// `flora:green`, ...
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            Theme::Flat => "flat",
            Theme::Flora => "flora",
            Theme::FloraGreen => "flora:green",
            Theme::FloraRed => "flora:red",
            Theme::FloraPurple => "flora:purple",
            Theme::FloraGold => "flora:gold",
            Theme::FloraRose => "flora:rose",
        }
    }

    /// The label on the settings page.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            Theme::Flat => "Flat",
            Theme::Flora => "Flora",
            Theme::FloraGreen => "Flora, green",
            Theme::FloraRed => "Flora, red",
            Theme::FloraPurple => "Flora, purple",
            Theme::FloraGold => "Flora, gold",
            Theme::FloraRose => "Flora, rose",
        }
    }

    /// Its name on the settings page, a key of appkit's resources (`kit-theme-flora-green`).
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            Theme::Flat => "kit-theme-flat",
            Theme::Flora => "kit-theme-flora",
            Theme::FloraGreen => "kit-theme-flora-green",
            Theme::FloraRed => "kit-theme-flora-red",
            Theme::FloraPurple => "kit-theme-flora-purple",
            Theme::FloraGold => "kit-theme-flora-gold",
            Theme::FloraRose => "kit-theme-flora-rose",
        }
    }

    /// The stone's name in the settings page's stone picker, a key of appkit's resources ("" for
    /// flat, which has none).
    #[must_use]
    pub fn stone_key(self) -> &'static str {
        match self {
            Theme::Flat => "",
            Theme::Flora => "kit-stone-blue",
            Theme::FloraGreen => "kit-stone-green",
            Theme::FloraRed => "kit-stone-red",
            Theme::FloraPurple => "kit-stone-purple",
            Theme::FloraGold => "kit-stone-gold",
            Theme::FloraRose => "kit-stone-rose",
        }
    }

    /// The stone's own label in the settings page's stone picker ("" for
    /// flat, which has none).
    #[must_use]
    pub fn stone_label(self) -> &'static str {
        match self {
            Theme::Flat => "",
            Theme::Flora => "Blue",
            Theme::FloraGreen => "Green",
            Theme::FloraRed => "Red",
            Theme::FloraPurple => "Purple",
            Theme::FloraGold => "Gold",
            Theme::FloraRose => "Rose",
        }
    }

    /// Whether this is flora or one of its spins.
    #[must_use]
    pub fn is_flora(self) -> bool {
        self != Theme::Flat
    }

    /// The position among [`Theme::FLORA`] (0 = blue); `None` for flat.
    #[must_use]
    pub fn stone_index(self) -> Option<usize> {
        Theme::FLORA.iter().position(|t| *t == self)
    }

    /// A theme by name, any case, surrounding blanks ignored (`flora:blue`
    /// is flora).
    #[must_use]
    pub fn parse(name: &str) -> Option<Theme> {
        let name = name.trim();
        if name.eq_ignore_ascii_case("flora:blue") {
            return Some(Theme::Flora);
        }
        Theme::ALL
            .into_iter()
            .find(|t| t.name().eq_ignore_ascii_case(name))
    }

    /// The position in [`Theme::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        Theme::ALL.iter().position(|t| *t == self).unwrap_or(0)
    }
}

/// Light, dark, or whatever the operating system uses.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum ModePref {
    #[default]
    System,
    Light,
    Dark,
}

impl ModePref {
    /// Every choice, in the order the settings page lists them.
    pub const ALL: [ModePref; 3] = [ModePref::System, ModePref::Light, ModePref::Dark];

    /// `system`, `light`, `dark`.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            ModePref::System => "system",
            ModePref::Light => "light",
            ModePref::Dark => "dark",
        }
    }

    /// The label on the settings page.
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            ModePref::System => "System",
            ModePref::Light => "Light",
            ModePref::Dark => "Dark",
        }
    }

    /// Its name on the settings page, a key of appkit's resources.
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            ModePref::System => "kit-mode-system",
            ModePref::Light => "kit-mode-light",
            ModePref::Dark => "kit-mode-dark",
        }
    }

    /// A choice by name, any case, surrounding blanks ignored.
    #[must_use]
    pub fn parse(name: &str) -> Option<ModePref> {
        let name = name.trim();
        ModePref::ALL
            .into_iter()
            .find(|m| m.name().eq_ignore_ascii_case(name))
    }

    /// The position in [`ModePref::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        ModePref::ALL.iter().position(|m| *m == self).unwrap_or(0)
    }
}

/// The language of the app's words: the system's, English or German (the settings page's
/// Language row, `--language`).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub enum LanguagePref {
    /// The language the engine finds on the system.
    #[default]
    System,
    English,
    German,
}

impl LanguagePref {
    pub const ALL: [LanguagePref; 3] =
        [LanguagePref::System, LanguagePref::English, LanguagePref::German];

    /// The name in the settings file and on the command line.
    #[must_use]
    pub fn name(self) -> &'static str {
        match self {
            LanguagePref::System => "system",
            LanguagePref::English => "en",
            LanguagePref::German => "de",
        }
    }

    /// The locale the engine is told (`CallbackInfo::set_locale`); `""` follows the system's
    /// language.
    #[must_use]
    pub fn tag(self) -> &'static str {
        match self {
            LanguagePref::System => "",
            LanguagePref::English => "en-US",
            LanguagePref::German => "de-DE",
        }
    }

    /// A choice by name (`system`, `en`, `de`) or by its own word (`English`, `Deutsch`), any
    /// case, surrounding blanks ignored.
    #[must_use]
    pub fn parse(name: &str) -> Option<LanguagePref> {
        let name = name.trim();
        LanguagePref::ALL.into_iter().find(|l| {
            l.name().eq_ignore_ascii_case(name)
                || match l {
                    LanguagePref::System => false,
                    LanguagePref::English => name.eq_ignore_ascii_case("english"),
                    LanguagePref::German => name.eq_ignore_ascii_case("deutsch"),
                }
        })
    }

    /// The position in [`LanguagePref::ALL`].
    #[must_use]
    pub fn index(self) -> usize {
        LanguagePref::ALL.iter().position(|l| *l == self).unwrap_or(0)
    }

    /// Its name on the settings page, a key of appkit's resources (English and German in their
    /// own words in either language).
    #[must_use]
    pub fn key(self) -> &'static str {
        match self {
            LanguagePref::System => "kit-language-system",
            LanguagePref::English => "kit-language-english",
            LanguagePref::German => "kit-language-german",
        }
    }
}

/// What an app tells the parser about itself.
#[derive(Clone, Copy, Debug)]
pub struct AppSpec {
    /// The product name, `AzCalculator`.
    pub name: &'static str,
    /// The program name in the usage line.
    pub binary: &'static str,
    /// One line under the name.
    pub summary: &'static str,
    /// The names `--screen` accepts, the first is the default screen.
    pub screens: &'static [&'static str],
    /// What bare arguments mean, for the usage text ("" = none accepted).
    pub files_help: &'static str,
}

/// The parsed switches.
#[derive(Clone, Debug, PartialEq, Default)]
pub struct AppArgs {
    /// `--screen`: one of [`AppSpec::screens`].
    pub screen: Option<String>,
    /// `--size WxH`, logical pixels.
    pub size: Option<(f32, f32)>,
    /// `--theme`.
    pub theme: Option<Theme>,
    /// `--mode`.
    pub mode: Option<ModePref>,
    /// `--language`: the run's language over the settings file's.
    pub language: Option<LanguagePref>,
    /// `--shot`: the PNG to write before exiting.
    pub shot: Option<PathBuf>,
    /// `--shot-delay-ms`: settle time before the screenshot.
    pub shot_delay_ms: u64,
    /// `--sample`: fill an empty data folder with sample data.
    pub sample: bool,
    /// `--data-dir`: the data root instead of the user's.
    pub data_dir: Option<PathBuf>,
    /// Bare arguments.
    pub files: Vec<PathBuf>,
}

/// The settle time before `--shot` when `--shot-delay-ms` is not given.
pub const DEFAULT_SHOT_DELAY_MS: u64 = 1500;

/// The usage text of an app.
#[must_use]
pub fn help(spec: &AppSpec) -> String {
    let files = if spec.files_help.is_empty() {
        String::new()
    } else {
        " [FILE...]".to_string()
    };
    let mut out = format!(
        "{name} - {summary}\n\nUSAGE:\n    {binary} [OPTIONS]{files}\n\nOPTIONS:\n",
        name = spec.name,
        summary = spec.summary,
        binary = spec.binary,
    );
    out.push_str(&format!(
        "    --screen <NAME>          {}\n",
        spec.screens.join(" | ")
    ));
    out.push_str("    --size <WxH>             Initial window size, e.g. --size 900x640\n");
    out.push_str("    --theme <NAME>           flat | flora | flora:green|red|purple|gold|rose (this run only)\n");
    out.push_str("    --mode <NAME>            system | light | dark (this run only)\n");
    out.push_str("    --language <NAME>        system | en | de (this run only)\n");
    out.push_str("    --shot <PNG>             Render, write this screenshot, exit\n");
    out.push_str(&format!(
        "    --shot-delay-ms <MS>     Settle time before --shot (default {DEFAULT_SHOT_DELAY_MS})\n"
    ));
    out.push_str("    --sample                 Fill an empty data folder with sample data\n");
    out.push_str(
        "    --data-dir <DIR>         The data root (default: $AZLIN_DATA, else the user's)\n",
    );
    out.push_str("    -h, --help               Print this help\n");
    if !spec.files_help.is_empty() {
        out.push_str(&format!("\nFILE: {}\n", spec.files_help));
    }
    out
}

impl AppArgs {
    /// Parses `argv` WITHOUT the program name. `Err` carries the message to
    /// print: the usage for `-h` / `--help`, otherwise what was wrong.
    pub fn parse<I, S>(spec: &AppSpec, argv: I) -> Result<AppArgs, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut a = AppArgs {
            shot_delay_ms: DEFAULT_SHOT_DELAY_MS,
            ..AppArgs::default()
        };
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
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
                "-h" | "--help" => return Err(help(spec)),
                "--screen" => {
                    let v = value("name")?;
                    match spec
                        .screens
                        .iter()
                        .find(|s| s.eq_ignore_ascii_case(v.trim()))
                    {
                        Some(screen) => a.screen = Some((*screen).to_string()),
                        None => {
                            return Err(format!(
                                "--screen: expected {}, got {v:?}",
                                spec.screens.join("|")
                            ))
                        }
                    }
                }
                "--size" => {
                    let v = value("WxH")?;
                    a.size = Some(parse_size(&v)?);
                }
                "--theme" => {
                    let v = value("name")?;
                    a.theme = Some(
                        Theme::parse(&v)
                            .ok_or_else(|| format!("--theme: expected flat|flora|flora:<green|red|purple|gold|rose>, got {v:?}"))?,
                    );
                }
                "--mode" => {
                    let v = value("name")?;
                    a.mode =
                        Some(ModePref::parse(&v).ok_or_else(|| {
                            format!("--mode: expected system|light|dark, got {v:?}")
                        })?);
                }
                "--language" => {
                    let v = value("name")?;
                    a.language = Some(LanguagePref::parse(&v).ok_or_else(|| {
                        format!("--language: expected system|en|de, got {v:?}")
                    })?);
                }
                "--shot" => a.shot = Some(PathBuf::from(value("path")?)),
                "--shot-delay-ms" => {
                    let v = value("number")?;
                    a.shot_delay_ms = v
                        .trim()
                        .parse()
                        .map_err(|_| format!("--shot-delay-ms: expected a number, got {v:?}"))?;
                }
                "--sample" => {
                    if inline.is_some() {
                        return Err("--sample takes no value".to_string());
                    }
                    a.sample = true;
                }
                "--data-dir" => {
                    let v = value("folder")?;
                    if v.trim().is_empty() {
                        return Err("--data-dir needs a folder".to_string());
                    }
                    a.data_dir = Some(PathBuf::from(v));
                }
                other if other.starts_with('-') && other != "-" => {
                    return Err(format!("unknown option {other:?}\n\n{}", help(spec)))
                }
                positional => {
                    if spec.files_help.is_empty() {
                        return Err(format!(
                            "{} takes no files ({positional:?})\n\n{}",
                            spec.name,
                            help(spec)
                        ));
                    }
                    a.files.push(PathBuf::from(positional));
                }
            }
            i += 1;
        }
        Ok(a)
    }

    /// The switches of this process (`std::env::args`, without the program name).
    pub fn from_env(spec: &AppSpec) -> Result<AppArgs, String> {
        AppArgs::parse(spec, std::env::args().skip(1))
    }

    /// The screen to open: `--screen`, else the app's first screen.
    #[must_use]
    pub fn screen_or_default<'a>(&'a self, spec: &'a AppSpec) -> &'a str {
        self.screen
            .as_deref()
            .or_else(|| spec.screens.first().copied())
            .unwrap_or("")
    }
}

/// `WxH` in logical pixels, both positive.
fn parse_size(v: &str) -> Result<(f32, f32), String> {
    let bad = || format!("--size: expected WxH in pixels, got {v:?}");
    let (w, h) = v.trim().split_once(['x', 'X']).ok_or_else(bad)?;
    match (w.trim().parse::<f32>(), h.trim().parse::<f32>()) {
        (Ok(w), Ok(h)) if w > 0.0 && h > 0.0 && w.is_finite() && h.is_finite() => Ok((w, h)),
        _ => Err(bad()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SPEC: AppSpec = AppSpec {
        name: "AzTest",
        binary: "AzTest",
        summary: "a test app",
        screens: &["main", "settings", "about"],
        files_help: "files to open",
    };

    const NO_FILES: AppSpec = AppSpec {
        files_help: "",
        ..SPEC
    };

    fn parse(args: &[&str]) -> Result<AppArgs, String> {
        AppArgs::parse(&SPEC, args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_first_screen_with_the_saved_theme_and_mode() {
        let a = parse(&[]).unwrap();
        assert_eq!(a.screen, None);
        assert_eq!(a.screen_or_default(&SPEC), "main");
        assert_eq!(a.theme, None, "no switch: the settings file decides");
        assert_eq!(a.mode, None);
        assert!(!a.sample && a.shot.is_none() && a.data_dir.is_none() && a.files.is_empty());
        assert_eq!(a.shot_delay_ms, DEFAULT_SHOT_DELAY_MS);
    }

    #[test]
    fn a_language_switch_names_the_language_of_the_run() {
        assert_eq!(parse(&[]).unwrap().language, None, "the settings decide");
        assert_eq!(
            parse(&["--language", "en"]).unwrap().language,
            Some(LanguagePref::English)
        );
        assert_eq!(
            parse(&["--language=system"]).unwrap().language,
            Some(LanguagePref::System)
        );
        assert!(parse(&["--language", "klingon"])
            .unwrap_err()
            .contains("system|en|de"));
    }

    #[test]
    fn both_spellings_of_a_valued_switch_work() {
        assert_eq!(
            parse(&["--size", "800x600"]).unwrap().size,
            Some((800.0, 600.0))
        );
        assert_eq!(
            parse(&["--size=800x600"]).unwrap().size,
            Some((800.0, 600.0))
        );
        assert_eq!(parse(&["--theme=flora"]).unwrap().theme, Some(Theme::Flora));
        assert_eq!(
            parse(&["--mode", "dark"]).unwrap().mode,
            Some(ModePref::Dark)
        );
        assert_eq!(parse(&["--shot-delay-ms=10"]).unwrap().shot_delay_ms, 10);
    }

    #[test]
    fn the_screen_must_be_one_of_the_apps_screens() {
        assert_eq!(
            parse(&["--screen", "Settings"]).unwrap().screen.as_deref(),
            Some("settings"),
            "any case, stored as the app spells it"
        );
        assert!(parse(&["--screen", "nope"]).is_err());
    }

    #[test]
    fn the_theme_and_the_mode_are_names_not_numbers() {
        assert_eq!(Theme::parse(" FLAT "), Some(Theme::Flat));
        assert_eq!(Theme::parse("native"), None);
        assert_eq!(ModePref::parse("System"), Some(ModePref::System));
        assert!(
            parse(&["--theme", "dark"]).is_err(),
            "dark is a mode, not a theme"
        );
        assert!(
            parse(&["--mode", "flora"]).is_err(),
            "flora is a theme, not a mode"
        );
        assert_eq!(Theme::Flora.name(), "flora");
        assert_eq!(ModePref::Light.name(), "light");
        assert_eq!(Theme::Flora.index(), 1);
        assert_eq!(Theme::parse("flora:green"), Some(Theme::FloraGreen));
        assert_eq!(Theme::parse("FLORA:BLUE"), Some(Theme::Flora));
        for t in Theme::ALL {
            assert_eq!(Theme::parse(t.name()), Some(t), "{t:?} round-trips");
        }
        assert_eq!(Theme::FloraRose.stone_index(), Some(5));
        assert_eq!(Theme::Flat.stone_index(), None);
        assert_eq!(ModePref::Dark.index(), 2);
    }

    #[test]
    fn sample_shot_and_data_dir_are_read() {
        let a = parse(&["--sample", "--shot", "out.png", "--data-dir", "/tmp/azlin"]).unwrap();
        assert!(a.sample);
        assert_eq!(a.shot, Some(PathBuf::from("out.png")));
        assert_eq!(a.data_dir, Some(PathBuf::from("/tmp/azlin")));
        assert!(parse(&["--sample=yes"]).is_err(), "a flag takes no value");
        assert!(
            parse(&["--data-dir="]).is_err(),
            "an empty folder is a mistake"
        );
    }

    #[test]
    fn bare_arguments_are_files_when_the_app_takes_files() {
        let a = parse(&["a.vcf", "--sample", "b.vcf"]).unwrap();
        assert_eq!(
            a.files,
            vec![PathBuf::from("a.vcf"), PathBuf::from("b.vcf")]
        );
        assert!(AppArgs::parse(&NO_FILES, ["a.vcf"]).is_err());
    }

    #[test]
    fn a_bad_switch_is_rejected_rather_than_ignored() {
        for bad in [
            "--sise=1x1",
            "--size=wide",
            "--size=0x10",
            "--size=-5x10",
            "--shot-delay-ms=soon",
            "--nonsense",
        ] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(
            parse(&["--shot"]).is_err(),
            "a switch missing its value is an error"
        );
    }

    #[test]
    fn help_is_an_error_carrying_the_usage_with_the_screens() {
        for flag in ["-h", "--help"] {
            let e = parse(&[flag]).unwrap_err();
            assert!(e.contains("USAGE"), "{flag} must print the usage");
            assert!(e.contains("main | settings | about"));
            assert!(e.contains("[FILE...]"));
        }
        assert!(!help(&NO_FILES).contains("[FILE...]"));
    }
}
