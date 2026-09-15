use std::path::PathBuf;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum FrameLog {
    #[default]
    Off,
    Slow,
    All,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Screen {
    #[default]
    Editor,
    BackstageInfo,
    BackstageOpen,
}

#[derive(Clone, Debug, Default)]
pub struct Args {
    pub open: Option<PathBuf>,
    pub screen: Screen,
    pub size: Option<(f32, f32)>,
    pub shot: Option<PathBuf>,
    pub shot_delay_ms: u64,
    pub paginate_twice: bool,
    pub frame_log: FrameLog,
    pub dump_xml: Option<PathBuf>,
}

const DEFAULT_SHOT_DELAY_MS: u64 = 2500;

pub const HELP: &str = "\
azwriter - a print-layout document editor demo

USAGE:
    azwriter [OPTIONS] [FILE.md]

OPTIONS:
    --open <FILE>            Markdown file to open (same as the positional form)
    --screen <NAME>          editor | backstage-info | backstage-open
    --size <WxH>             Initial window size, e.g. --size 1280x800
    --shot <PNG>             Render, write this screenshot, exit
    --shot-delay-ms <MS>     Settle time before --shot (default 2500)
    --paginate-twice         Paginate twice at startup (cold/warm comparison)
    --frame-log <MODE>       off | slow | all
    --dump-xml <FILE>        Write the generated document XML here
    -h, --help               Print this help
";

pub type ParseError = String;

impl Args {
    pub fn parse<I, S>(argv: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut a = Self {
            shot_delay_ms: DEFAULT_SHOT_DELAY_MS,
            ..Self::default()
        };
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (arg, None),
            };
            let mut value = |what: &str| -> Result<String, ParseError> {
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
                "--open" => a.open = Some(PathBuf::from(value("file")?)),
                "--screen" => {
                    let v = value("name")?;
                    a.screen = match v.as_str() {
                        "editor" => Screen::Editor,
                        "backstage-info" => Screen::BackstageInfo,
                        "backstage-open" => Screen::BackstageOpen,
                        other => {
                            return Err(format!(
                                "--screen: expected editor|backstage-info|backstage-open, got \
                                 {other:?}"
                            ))
                        }
                    };
                }
                "--size" => {
                    let v = value("WxH")?;
                    let (w, h) = v
                        .split_once('x')
                        .ok_or_else(|| format!("--size: expected WxH, got {v:?}"))?;
                    let (w, h) = (w.parse::<f32>(), h.parse::<f32>());
                    match (w, h) {
                        (Ok(w), Ok(h)) if w > 0.0 && h > 0.0 => a.size = Some((w, h)),
                        _ => return Err(format!("--size: expected WxH in pixels, got {v:?}")),
                    }
                }
                "--shot" => a.shot = Some(PathBuf::from(value("path")?)),
                "--shot-delay-ms" => {
                    let v = value("number")?;
                    a.shot_delay_ms = v
                        .parse()
                        .map_err(|_| format!("--shot-delay-ms: expected a number, got {v:?}"))?;
                }
                "--paginate-twice" => a.paginate_twice = true,
                "--frame-log" => {
                    let v = value("mode")?;
                    a.frame_log = match v.as_str() {
                        "off" => FrameLog::Off,
                        "slow" => FrameLog::Slow,
                        "all" => FrameLog::All,
                        other => {
                            return Err(format!(
                                "--frame-log: expected off|slow|all, got {other:?}"
                            ))
                        }
                    };
                }
                "--dump-xml" => a.dump_xml = Some(PathBuf::from(value("path")?)),
                other if other.starts_with('-') => {
                    return Err(format!("unknown option {other:?}\n\n{HELP}"))
                }
                positional => {
                    if a.open.is_some() {
                        return Err(format!("more than one file given ({positional:?})"));
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

    fn parse(args: &[&str]) -> Result<Args, ParseError> {
        Args::parse(args.iter().copied())
    }

    #[test]
    fn no_arguments_is_the_plain_editor() {
        let a = parse(&[]).unwrap();
        assert_eq!(a.screen, Screen::Editor);
        assert!(a.open.is_none() && a.shot.is_none());
        assert_eq!(a.frame_log, FrameLog::Off);
        assert_eq!(a.shot_delay_ms, DEFAULT_SHOT_DELAY_MS);
    }

    #[test]
    fn a_bare_path_is_the_document_to_open() {
        assert_eq!(
            parse(&["notes.md"]).unwrap().open,
            Some(PathBuf::from("notes.md"))
        );
        assert_eq!(
            parse(&["--open", "notes.md"]).unwrap().open,
            Some(PathBuf::from("notes.md"))
        );
        assert!(parse(&["a.md", "b.md"]).is_err(), "two files is a mistake");
    }

    #[test]
    fn both_spellings_of_a_valued_flag_work() {
        assert_eq!(
            parse(&["--size", "800x600"]).unwrap().size,
            Some((800.0, 600.0))
        );
        assert_eq!(
            parse(&["--size=800x600"]).unwrap().size,
            Some((800.0, 600.0))
        );
        assert_eq!(parse(&["--shot-delay-ms=10"]).unwrap().shot_delay_ms, 10);
    }

    #[test]
    fn a_bad_option_is_rejected_rather_than_ignored() {
        for bad in [
            "--shot-dely-ms=1",
            "--screen=backstage",
            "--size=wide",
            "--frame-log=verbose",
            "--nonsense",
        ] {
            assert!(parse(&[bad]).is_err(), "{bad} must be rejected");
        }
        assert!(
            parse(&["--shot"]).is_err(),
            "a flag missing its value is an error, not an empty path"
        );
    }

    #[test]
    fn the_screens_and_log_modes_map_by_name() {
        assert_eq!(
            parse(&["--screen", "backstage-open"]).unwrap().screen,
            Screen::BackstageOpen
        );
        assert_eq!(
            parse(&["--screen", "backstage-info"]).unwrap().screen,
            Screen::BackstageInfo
        );
        assert_eq!(
            parse(&["--frame-log", "all"]).unwrap().frame_log,
            FrameLog::All
        );
        assert_eq!(
            parse(&["--frame-log", "slow"]).unwrap().frame_log,
            FrameLog::Slow
        );
    }

    #[test]
    fn help_is_an_error_carrying_the_help_text() {
        for flag in ["-h", "--help"] {
            let e = parse(&[flag]).unwrap_err();
            assert!(e.contains("USAGE"), "{flag} must print the usage");
        }
    }
}
