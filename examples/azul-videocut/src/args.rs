//! The command line: `--sample`, `--project <uuid>`, `--screen <name>`,
//! `--theme <flat|flora>`, `--mode <light|dark>`, `--size <WxH>`.

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

/// Light or dark, forced from the command line.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mode {
    Light,
    Dark,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Args {
    /// Make the sample project (generated clips; encoded to MP4 where the
    /// machine has an H.264 encoder) and open it.
    pub sample: bool,
    /// Open this project.
    pub project: Option<String>,
    pub screen: Screen,
    /// "flat" or "flora".
    pub theme: Option<String>,
    pub mode: Option<Mode>,
    pub size: Option<(f32, f32)>,
}

pub const HELP: &str = "\
AzVideoCut - a video editor on azul's video stack

USAGE:
    AzVideoCut [OPTIONS]

OPTIONS:
    --sample                 Make the sample project and open it
    --project <UUID>         Open this project (videocut/<UUID>/project.json)
    --screen <NAME>          editor | export | settings | about
    --theme <NAME>           flat | flora
    --mode <NAME>            light | dark
    --size <WxH>             Initial window size, e.g. --size 1280x800
    -h, --help               Print this help

ENVIRONMENT:
    AZVIDEOCUT_DATA          The data root (default: <data dir>/azul)
";

pub type ParseError = String;

impl Args {
    pub fn parse<I, S>(argv: I) -> Result<Self, ParseError>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let mut a = Self::default();
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
                "--sample" => a.sample = true,
                "--project" => a.project = Some(value("uuid")?),
                "--screen" => {
                    let v = value("name")?;
                    a.screen = match v.as_str() {
                        "editor" => Screen::Editor,
                        "export" => Screen::Export,
                        "settings" => Screen::Settings,
                        "about" => Screen::About,
                        other => {
                            return Err(format!(
                                "--screen: expected editor|export|settings|about, got {other:?}"
                            ))
                        }
                    };
                }
                "--theme" => {
                    let v = value("name")?;
                    if v != "flat" && v != "flora" {
                        return Err(format!("--theme: expected flat|flora, got {v:?}"));
                    }
                    a.theme = Some(v);
                }
                "--mode" => {
                    let v = value("name")?;
                    a.mode = Some(match v.as_str() {
                        "light" => Mode::Light,
                        "dark" => Mode::Dark,
                        other => return Err(format!("--mode: expected light|dark, got {other:?}")),
                    });
                }
                "--size" => {
                    let v = value("WxH")?;
                    let (w, h) = v
                        .split_once('x')
                        .ok_or_else(|| format!("--size: expected WxH, got {v:?}"))?;
                    match (w.parse::<f32>(), h.parse::<f32>()) {
                        (Ok(w), Ok(h)) if w > 0.0 && h > 0.0 => a.size = Some((w, h)),
                        _ => return Err(format!("--size: expected WxH in pixels, got {v:?}")),
                    }
                }
                other => return Err(format!("unknown option {other:?}\n\n{HELP}")),
            }
            i += 1;
        }
        Ok(a)
    }
}

#[cfg(test)]
#[path = "args_tests.rs"]
mod tests;
