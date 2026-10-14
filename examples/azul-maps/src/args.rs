//! AzMaps' command line: azul-appkit's switches (`--theme`, `--mode`, `--size`, `--shot`,
//! `--sample`, `--data-dir`) plus AzMaps' own `--stats`, `--tiles` and `--routing`; `-h` / `--help`. Pure:
//! no azul types, tested without a window.

use azul_appkit::{args::help, AppArgs};

/// What the command line asked for.
#[derive(Debug, Clone, PartialEq, Default)]
pub struct Args {
    /// `--stats` (else `AZMAPS_STATS=1`): the map's counters on stdout -
    /// `AZ_MAP_TILES` / `AZ_MAP_RENDER` from the map widget, `AZ_MAP_TILE`
    /// from its tile worker, `AZMAPS_LAYOUT` for every window rebuild and
    /// `AZMAPS_PAN` for every step of a pan.
    pub stats: bool,
    /// `--tiles <url template>`: the tile server for this run (see [`tile_template`]).
    pub tiles: Option<String>,
    /// `--routing <url template or folder>`: the routing tiles for this run, else
    /// [`ROUTING_VAR`], else the shared Azlin config's `endpoints.routing`; without them a
    /// route is the straight-line estimate.
    pub routing: Option<String>,
    /// `-h` / `--help`.
    pub help: bool,
    /// The Azlin switches (azul-appkit).
    pub kit: AppArgs,
}

/// The variable naming the tile server, the fallback for `--tiles`.
pub const TILES_VAR: &str = "AZMAPS_TILES";
/// The variable naming the routing tiles, the fallback for `--routing`.
pub const ROUTING_VAR: &str = "AZMAPS_ROUTING";

/// The tile server's URL template (`{z}`, `{x}`, `{y}`): `--tiles`, else
/// [`TILES_VAR`], else the shared Azlin config's `endpoints.tiles`; `None`
/// keeps the map widget's own (OpenFreeMap's public planet tiles).
#[must_use]
pub fn tile_template(
    flag: Option<&str>,
    env: Option<&str>,
    shared: Option<&str>,
) -> Option<String> {
    [flag, env, shared]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|t| !t.is_empty())
        .map(String::from)
}

/// The usage text: appkit's, then AzMaps' own switches.
#[must_use]
pub fn usage() -> String {
    let mut text = help(&crate::SPEC);
    text.push_str(
        "\nAZMAPS:\n    --stats                  Print the map's counters on stdout: AZ_MAP_TILES \
         (tiles\n                             by state), AZ_MAP_RENDER (each render of the \
         tiles),\n                             AZ_MAP_TILE (each tile's fetch / decode / draw \
         ms),\n                             AZMAPS_LAYOUT (each window rebuild, us), \
         AZMAPS_PAN\n                             (each step of a pan: map, move or \
         rebuild, us).\n                             Also AZMAPS_STATS=1\n    --tiles <URL>            The tile server's URL template \
         ({z}/{x}/{y}) for this run; else\n                             AZMAPS_TILES, else \
         endpoints.tiles of the shared Azlin config,\n                             else \
         OpenFreeMap's public tiles\n    --routing <URL|DIR>      The routing tiles ({z}/{x}/{y}.json.gz, or a folder\n                             azul-routing's routing-tiles wrote); else \
         AZMAPS_ROUTING, else\n                             endpoints.routing of the shared Azlin \
         config. Without them a\n                             route is the straight-line estimate\n",
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
    let mut tiles = None;
    let mut routing = None;
    let mut rest = Vec::with_capacity(argv.len());
    let mut argv = argv.into_iter();
    while let Some(arg) = argv.next() {
        if arg == "--stats" {
            stats = true;
        } else if arg.starts_with("--stats=") {
            return Err("--stats takes no value".to_string());
        } else if arg == "--tiles" {
            tiles = Some(
                argv.next()
                    .ok_or_else(|| "--tiles needs a URL template".to_string())?,
            );
        } else if let Some(template) = arg.strip_prefix("--tiles=") {
            tiles = Some(template.to_string());
        } else if arg == "--routing" {
            routing = Some(
                argv.next()
                    .ok_or_else(|| "--routing needs a URL template or a folder".to_string())?,
            );
        } else if let Some(template) = arg.strip_prefix("--routing=") {
            routing = Some(template.to_string());
        } else {
            rest.push(arg);
        }
    }
    Ok(Args {
        stats,
        tiles,
        routing,
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

    #[test]
    fn the_tiles_switch_names_the_tile_server_for_this_run() {
        let local = "http://127.0.0.1:8099/tiles/{z}/{x}/{y}.pbf";
        let a = parse(["--tiles", local, "--stats"], false).expect("valid");
        assert_eq!(a.tiles.as_deref(), Some(local));
        assert!(a.stats);
        let a = parse([format!("--tiles={local}")], false).expect("valid");
        assert_eq!(a.tiles.as_deref(), Some(local));
        assert!(parse(["--tiles"], false).is_err(), "--tiles needs a template");
        assert!(usage().contains("--tiles"));
    }

    #[test]
    fn the_routing_switch_names_the_routing_tiles_a_url_or_a_folder() {
        let local = "http://127.0.0.1:8765/routing/{z}/{x}/{y}.json.gz";
        let a = parse(["--routing", local], false).expect("valid");
        assert_eq!(a.routing.as_deref(), Some(local));
        let a = parse(["--routing=/tmp/routing-tiles"], false).expect("valid");
        assert_eq!(a.routing.as_deref(), Some("/tmp/routing-tiles"));
        assert_eq!(parse(Vec::<String>::new(), false).expect("valid").routing, None);
        assert!(parse(["--routing"], false).is_err(), "--routing needs a value");
        assert!(usage().contains("--routing"));
    }

    #[test]
    fn the_switch_wins_over_the_variable_and_the_variable_over_the_shared_config() {
        let flag = "http://127.0.0.1:8099/a/{z}/{x}/{y}.pbf";
        let env = "http://127.0.0.1:8099/b/{z}/{x}/{y}.pbf";
        let shared = "http://127.0.0.1:8099/c/{z}/{x}/{y}.pbf";
        assert_eq!(
            tile_template(Some(flag), Some(env), Some(shared)).as_deref(),
            Some(flag)
        );
        assert_eq!(
            tile_template(None, Some(env), Some(shared)).as_deref(),
            Some(env)
        );
        assert_eq!(
            tile_template(None, Some("  "), Some(shared)).as_deref(),
            Some(shared),
            "a blank variable counts as unset"
        );
        assert_eq!(
            tile_template(None, None, None),
            None,
            "the widget's own tiles"
        );
    }
}
