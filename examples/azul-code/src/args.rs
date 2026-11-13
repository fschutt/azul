//! AzCode's command line: the switches every Azlin app understands
//! (azul-appkit's `--screen`, `--size`, `--theme`, `--mode`, `--shot`,
//! `--sample`, `--data-dir`, a bare folder or file to open), plus AzCode's
//! own:
//!
//! - `--folder <DIR>`: the folder to open as the workspace (what Open Folder
//!   picks; a script's way past the folder dialog);
//! - `--shell <PROGRAM>`: what the terminal panel runs (default: the user's
//!   `$SHELL`); words after the program are its arguments
//!   (`--shell "bash --noprofile"`).

use std::path::PathBuf;

use azul_appkit::args::{help, AppArgs};

use crate::SPEC;

/// AzCode's own switches, after appkit's in the usage text.
const OWN_HELP: &str = "\nAZCODE:\n    --folder <DIR>           The folder to open as the workspace\n    \
                        --shell <PROGRAM>        What the terminal panel runs (default: $SHELL)\n";

/// The parsed command line.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Args {
    /// `--folder`.
    pub folder: Option<PathBuf>,
    /// `--shell`: the program and its arguments.
    pub shell: Option<(String, Vec<String>)>,
    /// The switches every Azlin app understands.
    pub kit: AppArgs,
}

/// The usage text: appkit's, then AzCode's own switches.
#[must_use]
pub fn usage() -> String {
    let mut text = help(&SPEC);
    text.push_str(OWN_HELP);
    text
}

/// `--shell`'s value as a program and its arguments (`None` for blanks).
#[must_use]
pub fn shell_of(value: &str) -> Option<(String, Vec<String>)> {
    let mut words = value.split_whitespace().map(str::to_string);
    let program = words.next()?;
    Some((program, words.collect()))
}

impl Args {
    /// Parses `argv` without the program name: AzCode's switches here, the
    /// rest by azul-appkit. `Err` carries the usage (`-h`) or what was
    /// wrong.
    pub fn parse<I, S>(argv: I) -> Result<Args, String>
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
        if argv.iter().any(|a| a == "-h" || a == "--help") {
            return Err(usage());
        }
        let mut folder = None;
        let mut shell = None;
        let mut rest = Vec::with_capacity(argv.len());
        let mut i = 0;
        while i < argv.len() {
            let arg = argv[i].as_str();
            let (name, inline) = match arg.split_once('=') {
                Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
                _ => (arg, None),
            };
            if name != "--folder" && name != "--shell" {
                rest.push(argv[i].clone());
                i += 1;
                continue;
            }
            let value = match inline {
                Some(v) => v,
                None => {
                    i += 1;
                    argv.get(i)
                        .cloned()
                        .ok_or_else(|| format!("{name} needs a value\n\n{}", usage()))?
                }
            };
            if name == "--folder" {
                if value.trim().is_empty() {
                    return Err("--folder needs a folder".to_string());
                }
                folder = Some(PathBuf::from(value));
            } else {
                shell = Some(shell_of(&value).ok_or_else(|| "--shell needs a program".to_string())?);
            }
            i += 1;
        }
        let kit = AppArgs::parse(&SPEC, rest)?;
        Ok(Args { folder, shell, kit })
    }

    /// The switches of this process (`std::env::args`, without the program
    /// name).
    pub fn from_env() -> Result<Args, String> {
        Args::parse(std::env::args().skip(1))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folder_and_shell_are_azcodes_and_the_rest_is_appkits() {
        let a = Args::parse(["--folder", "/tmp/project", "--shell=/bin/sh", "--mode", "dark"]).expect("valid");
        assert_eq!(a.folder, Some(PathBuf::from("/tmp/project")));
        assert_eq!(a.shell, Some(("/bin/sh".to_string(), Vec::new())));
        assert!(a.kit.mode.is_some(), "--mode is appkit's");
        let b = Args::parse(["--shell", "bash --noprofile -i", "notes"]).expect("valid");
        assert_eq!(
            b.shell,
            Some(("bash".to_string(), vec!["--noprofile".to_string(), "-i".to_string()]))
        );
        assert_eq!(b.kit.files, vec![PathBuf::from("notes")], "a bare folder is still a file to open");
        assert!(Args::parse(["--folder"]).is_err(), "a switch without its value");
        assert!(Args::parse(["--shell", "  "]).is_err());
        assert!(Args::parse(["--nonsense"]).is_err());
        let usage = Args::parse(["-h"]).expect_err("the usage");
        assert!(usage.contains("--folder <DIR>") && usage.contains("--shell <PROGRAM>") && usage.contains("--theme"));
    }
}
