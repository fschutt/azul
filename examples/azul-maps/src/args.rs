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
    /// `--routing <URL or folder>`: the routing tiles for this run (their manifest's URL, a
    /// `manifest.json` or the folder holding it; a `{z}/{x}/{y}` template still works), else
    /// [`ROUTING_VAR`], else the shared Azlin config's `endpoints.routing`; without them a
    /// route is asked of the routing server.
    pub routing: Option<String>,
    /// `--routing-online <url|off>`: the routing server asked where there are no routing
    /// tiles, else [`ROUTING_ONLINE_VAR`] (see [`online_routing`]).
    pub routing_online: Option<String>,
    /// `--transit <url|off>`: the public transport server (a MOTIS server such as Transitous),
    /// else [`TRANSIT_VAR`] (see [`transit_server`]).
    pub transit: Option<String>,
    /// `--offline`: nothing goes out; every online answer comes from the cache, marked offline.
    pub offline: bool,
    /// `--simulate-route`: a navigation is driven by a simulated car along its route (with one
    /// wrong turn, so a new route is asked for), not by the position.
    pub simulate: bool,
    /// `--simulate-speed <km/h>`: the simulated car's speed (else [`SIMULATED_KMH`]).
    pub simulate_kmh: Option<f64>,
    /// `--gps <source>`: where the position comes from for this run: `platform`, `gpsd`,
    /// `gpsd:<host:port>`, `nmea:<device>`, `replay:<file>` (else Settings > Location).
    pub gps: Option<String>,
    /// `--replay-speed <factor>`: how much faster an NMEA replay (`--gps replay:<file>`) runs
    /// than it was recorded (else 1).
    pub replay_speed: Option<f64>,
    /// `--import <file or folder>`: imported into the saved lists at the start.
    pub import: Option<String>,
    /// `--navigate`: navigation starts as soon as the road route is there.
    pub navigate: bool,
    /// `-h` / `--help`.
    pub help: bool,
    /// The Azlin switches (azul-appkit).
    pub kit: AppArgs,
}

/// The variable naming the tile server, the fallback for `--tiles`.
pub const TILES_VAR: &str = "AZMAPS_TILES";
/// The variable naming the routing tiles, the fallback for `--routing`.
pub const ROUTING_VAR: &str = "AZMAPS_ROUTING";
/// The variable naming the routing server, the fallback for `--routing-online`.
pub const ROUTING_ONLINE_VAR: &str = "AZMAPS_ROUTING_ONLINE";
/// The variable naming the public transport server, the fallback for `--transit`.
pub const TRANSIT_VAR: &str = "AZMAPS_TRANSIT";
/// The simulated car's speed without `--simulate-speed`, km/h.
pub const SIMULATED_KMH: f64 = 50.0;

/// The public transport server's base URL: `--transit`, else [`TRANSIT_VAR`], else
/// Transitous; `off` (or `none`) plans no journeys.
#[must_use]
pub fn transit_server(flag: Option<&str>, env: Option<&str>) -> Option<String> {
    let chosen = [flag, env]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|t| !t.is_empty())
        .unwrap_or(azul_routing::transit::transitous::TRANSITOUS);
    (!matches!(chosen.to_ascii_lowercase().as_str(), "off" | "none")).then(|| chosen.to_string())
}

/// The routing server's `/route` URL: `--routing-online`, else [`ROUTING_ONLINE_VAR`], else
/// FOSSGIS's public Valhalla server; `off` (or `none`) asks no server, so a route without
/// routing tiles is the straight-line estimate.
#[must_use]
pub fn online_routing(flag: Option<&str>, env: Option<&str>) -> Option<String> {
    let chosen = [flag, env]
        .into_iter()
        .flatten()
        .map(str::trim)
        .find(|t| !t.is_empty())
        .unwrap_or(azul_routing::online::FOSSGIS_VALHALLA);
    (!matches!(chosen.to_ascii_lowercase().as_str(), "off" | "none")).then(|| chosen.to_string())
}

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
         OpenFreeMap's public tiles\n    --routing <URL|PATH>     The routing tiles: their manifest's URL, a manifest.json\n                             or its folder (a {z}/{x}/{y} template works too); else \
         AZMAPS_ROUTING, else\n                             endpoints.routing of the shared Azlin \
         config. Without them a\n                             route is asked of the routing server\n    --routing-online <URL|off>  The routing server (a Valhalla /route URL) asked\n                             without routing tiles; else AZMAPS_ROUTING_ONLINE,\n                             else FOSSGIS's public server. off: the straight-line\n                             estimate\n    --transit <URL|off>      The public transport server (MOTIS); else\n                             AZMAPS_TRANSIT, else Transitous. off: no journeys\n    --offline                Send nothing: routes, journeys and places come from the\n                             cache, marked offline\n    --simulate-route         Navigation drives a simulated car along the route (one\n                             wrong turn on the way); --simulate-speed <km/h> (50)\n    --gps <SOURCE>           Where the position comes from: platform, gpsd,\n                             gpsd:<host:port>, nmea:<device>, replay:<file.nmea>\n    --replay-speed <N>       How much faster a replay runs than it was recorded (1)\n    --import <FILE|DIR>      Import places (GPX, KML, KMZ, GeoJSON, CSV, a Google\n                             Takeout folder) at the start\n    --navigate               Start navigating as soon as the road route is there\n",
    );
    text
}

/// A switch of `names` with its value (`--name value` or `--name=value`): the name and the
/// value; `None` for any other argument.
fn valued<'n>(
    arg: &str,
    rest: &mut impl Iterator<Item = String>,
    names: &[&'n str],
) -> Result<Option<(&'n str, String)>, String> {
    for name in names {
        if arg == *name {
            let value = rest.next().ok_or_else(|| format!("{name} needs a value"))?;
            return Ok(Some((*name, value)));
        }
        if let Some(value) = arg.strip_prefix(name).and_then(|v| v.strip_prefix('=')) {
            return Ok(Some((*name, value.to_string())));
        }
    }
    Ok(None)
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
    let mut routing_online = None;
    let mut transit = None;
    let mut offline = false;
    let mut simulate = false;
    let mut simulate_kmh = None;
    let mut gps = None;
    let mut replay_speed = None;
    let mut import = None;
    let mut navigate = false;
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
                    .ok_or_else(|| "--routing needs the tiles' manifest URL or a folder".to_string())?,
            );
        } else if arg == "--routing-online" {
            routing_online = Some(
                argv.next()
                    .ok_or_else(|| "--routing-online needs a URL or off".to_string())?,
            );
        } else if let Some(url) = arg.strip_prefix("--routing-online=") {
            routing_online = Some(url.to_string());
        } else if let Some(template) = arg.strip_prefix("--routing=") {
            routing = Some(template.to_string());
        } else if arg == "--offline" {
            offline = true;
        } else if arg == "--simulate-route" {
            simulate = true;
        } else if arg == "--navigate" {
            navigate = true;
        } else if let Some((name, value)) = valued(&arg, &mut argv, &["--transit", "--simulate-speed", "--replay-speed", "--gps", "--import"])? {
            match name {
                "--transit" => transit = Some(value),
                "--simulate-speed" => {
                    let kmh: f64 = value
                        .trim()
                        .parse()
                        .map_err(|_| format!("--simulate-speed needs km/h, not {value}"))?;
                    if !(kmh.is_finite() && kmh > 0.0) {
                        return Err(format!("--simulate-speed needs a speed above zero, not {value}"));
                    }
                    simulate_kmh = Some(kmh);
                }
                "--replay-speed" => {
                    let factor: f64 = value
                        .trim()
                        .parse()
                        .map_err(|_| format!("--replay-speed needs a factor, not {value}"))?;
                    if !(factor.is_finite() && factor > 0.0) {
                        return Err(format!("--replay-speed needs a factor above zero, not {value}"));
                    }
                    replay_speed = Some(factor);
                }
                "--gps" => gps = Some(value),
                _ => import = Some(value),
            }
        } else {
            rest.push(arg);
        }
    }
    Ok(Args {
        stats,
        tiles,
        routing,
        routing_online,
        transit,
        offline,
        simulate,
        simulate_kmh,
        gps,
        replay_speed,
        import,
        navigate,
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
    fn the_routing_server_is_fossgis_unless_one_is_named_or_it_is_off() {
        let fossgis = azul_routing::online::FOSSGIS_VALHALLA;
        assert_eq!(online_routing(None, None).as_deref(), Some(fossgis));
        assert_eq!(online_routing(Some("http://127.0.0.1:8002/route"), Some("off")).as_deref(), Some("http://127.0.0.1:8002/route"));
        assert_eq!(online_routing(None, Some("OFF")), None);
        assert_eq!(online_routing(Some("none"), None), None);
        let a = parse(["--routing-online", "off"], false).expect("valid");
        assert_eq!(a.routing_online.as_deref(), Some("off"));
        assert!(parse(["--routing-online"], false).is_err());
        assert!(usage().contains("--routing-online"));
    }

    #[test]
    fn the_navigation_switches_are_azmaps_own() {
        let a = parse(
            [
                "--transit",
                "http://127.0.0.1:8080",
                "--offline",
                "--simulate-route",
                "--simulate-speed",
                "120",
                "--gps",
                "replay:/tmp/drive.nmea",
                "--import=/tmp/Takeout",
                "--navigate",
            ],
            false,
        )
        .expect("valid");
        assert_eq!(a.transit.as_deref(), Some("http://127.0.0.1:8080"));
        assert!(a.offline && a.simulate && a.navigate);
        assert_eq!(a.simulate_kmh, Some(120.0));
        assert_eq!(a.gps.as_deref(), Some("replay:/tmp/drive.nmea"));
        assert_eq!(a.import.as_deref(), Some("/tmp/Takeout"));
        let r = parse(["--replay-speed", "8"], false).expect("valid");
        assert_eq!(r.replay_speed, Some(8.0));
        assert!(parse(["--replay-speed", "-1"], false).is_err());
        assert!(usage().contains("--replay-speed"));
        let none = parse(Vec::<String>::new(), false).expect("valid");
        assert!(!none.offline && !none.simulate && !none.navigate && none.gps.is_none());
        assert!(parse(["--simulate-speed", "fast"], false).is_err());
        assert!(parse(["--simulate-speed", "0"], false).is_err(), "a car that never moves");
        assert!(parse(["--gps"], false).is_err());
        for switch in ["--transit", "--offline", "--simulate-route", "--gps", "--import", "--navigate"] {
            assert!(usage().contains(switch), "{switch} is in the usage");
        }
    }

    #[test]
    fn the_transit_server_is_transitous_unless_one_is_named_or_it_is_off() {
        let transitous = azul_routing::transit::transitous::TRANSITOUS;
        assert_eq!(transit_server(None, None).as_deref(), Some(transitous));
        assert_eq!(transit_server(Some("http://127.0.0.1:8080/"), Some("off")).as_deref(), Some("http://127.0.0.1:8080/"));
        assert_eq!(transit_server(None, Some("off")), None);
        assert_eq!(transit_server(Some(" NONE "), None), None);
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
