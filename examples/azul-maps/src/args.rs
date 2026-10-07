//! AzMaps' command line: azul-appkit's switches (`--theme`, `--mode`, `--size`, `--shot`,
//! `--sample`, `--data-dir`) plus AzMaps' own `--stats`; `-h` / `--help`. Pure: no azul
//! types, tested without a window.

use azul_appkit::{args::help, AppArgs};

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Args {
    /// `--stats` (else `AZMAPS_STATS=1`): the map's counters on stdout -
    /// `AZ_MAP_TILES` / `AZ_MAP_RENDER` from the map widget, `AZ_MAP_TILE`
    /// from its tile worker, `AZMAPS_LAYOUT` for every window rebuild.
    pub stats: bool,
    /// `-h` / `--help`.
    pub help: bool,
    /// The Azlin switches (azul-appkit).
    pub kit: AppArgs,
}

/// The usage text: appkit's, then AzMaps' own switches.
#[must_use]
pub fn usage() -> String {
    let mut text = help(&crate::SPEC);
    text.push_str(
        "\nAZMAPS:\n    --stats                  Print the map's counters on stdout: AZ_MAP_TILES \
         (tiles\n                             by state), AZ_MAP_RENDER (each render of the \
         tiles),\n                             AZ_MAP_TILE (each tile's fetch / decode / draw \
         ms),\n                             AZMAPS_LAYOUT (each window rebuild). Also \
         AZMAPS_STATS=1\n",
    );
    text
}

/// Reads `argv` (without the program name); `env_stats` is whether
/// `AZMAPS_STATS` is set (`1`), the fallback for `--stats`. `--key value` and
/// `--key=value` both work for appkit's switches.
///
/// # Errors
/// What is wrong with the command line.
pub fn parse<I, S>(argv: I, env_stats: bool) -> Result<Args, String>
where
    I: IntoIterator<Item = S>,
    S: Into<String>,
{
    let argv: Vec<String> = argv.into_iter().map(Into::into).collect();
    if argv.iter().any(|a| a == "-h" || a == "--help") {
        return Ok(Args {
            help: true,
            ..Args::default()
        });
    }
    let mut stats = env_stats;
    let mut rest = Vec::with_capacity(argv.len());
    for arg in argv {
        if arg == "--stats" {
            stats = true;
        } else if arg.starts_with("--stats=") {
            return Err("--stats takes no value".to_string());
        } else {
            rest.push(arg);
        }
    }
    Ok(Args {
        stats,
        help: false,
        kit: AppArgs::parse(&crate::SPEC, rest)?,
    })
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
