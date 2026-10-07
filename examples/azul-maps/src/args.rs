//! AzMaps' command line: azul-appkit's switches (`--theme`, `--mode`, `--size`, `--shot`,
//! `--sample`, `--data-dir`) plus AzMaps' own `--stats`; `-h` / `--help`. Pure: no azul
//! types, tested without a window.

use azul_appkit::{args::help, AppArgs};

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Args {
    /// `--stats`: the map's counters on stdout.
    pub stats: bool,
    /// `-h` / `--help`.
    pub help: bool,
    /// The Azlin switches (azul-appkit).
    pub kit: AppArgs,
}

/// The usage text: appkit's, then AzMaps' own switches.
#[must_use]
pub fn usage() -> String {
    help(&crate::SPEC)
}

/// Reads `argv` (without the program name); `env_stats` is `AZMAPS_STATS=1`.
///
/// # Errors
/// What is wrong with the command line.
pub fn parse<I, S>(argv: I, env_stats: bool) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let _ = (argv.into_iter().count(), env_stats);
    todo!()
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    #[test]
    fn stats_is_azmaps_own_switch_and_every_other_one_is_appkits() {
        let a = parse(["--stats", "--data-dir", "/tmp/x", "--size=800x600"], false).expect("valid");
        assert!(a.stats && !a.help);
        assert_eq!(a.kit.data_dir, Some(PathBuf::from("/tmp/x")));
        assert_eq!(a.kit.size, Some((800.0, 600.0)));
        assert!(!parse(Vec::<String>::new(), false).expect("valid").stats, "off by default");
        assert!(
            parse(Vec::<String>::new(), true).expect("valid").stats,
            "AZMAPS_STATS=1 is the fallback"
        );
        assert!(parse(["-h"], false).expect("valid").help);
        assert!(parse(["--stats=yes"], false).is_err(), "--stats takes no value");
        assert!(parse(["--bogus"], false).is_err());
        assert!(usage().contains("--stats") && usage().contains("--data-dir"));
    }
}
