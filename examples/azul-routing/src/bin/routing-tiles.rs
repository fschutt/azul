//! `routing-tiles <extract.osm.pbf> <out-dir>`: cuts an OpenStreetMap
//! extract into AzMaps' routing tiles, `<out-dir>/<z>/<x>/<y>.json.gz` for
//! zoom 8, 11 and 14, ready for any static file hosting.
//!
//! `routing-tiles --route <tile-dir> <lat,lon> <lat,lon> [car|bike|walk]`
//! works out a route on such a folder and prints it, for a quick check of
//! the tiles without the app.

use std::{path::PathBuf, process::ExitCode};

use azul_routing::{
    generate::{generate, write_to_folder},
    route,
    source::FolderTiles,
    Mode,
};

fn place(text: &str) -> Option<(f64, f64)> {
    let (lat, lon) = text.split_once(',')?;
    Some((lat.trim().parse().ok()?, lon.trim().parse().ok()?))
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.as_slice() {
        [flag, dir, from, to, rest @ ..] if flag == "--route" => {
            let (Some(from), Some(to)) = (place(from), place(to)) else {
                eprintln!("a place is `lat,lon`");
                return ExitCode::from(2);
            };
            let mode = match rest.first().map(String::as_str) {
                None | Some("car") => Mode::Car,
                Some("bike") => Mode::Bike,
                Some("walk") => Mode::Walk,
                Some(other) => {
                    eprintln!("no mode `{other}`: car, bike or walk");
                    return ExitCode::from(2);
                }
            };
            let source = FolderTiles {
                root: PathBuf::from(dir),
            };
            match route(&source, from, to, mode, &mut |_| {}, &mut || false) {
                Ok(r) => {
                    println!(
                        "ROUTE {:.3} km {:.1} min {} points {} tiles {} expanded {} attempts",
                        r.distance_m / 1000.0,
                        r.duration_s / 60.0,
                        r.points.len(),
                        r.tiles,
                        r.expanded,
                        r.attempts
                    );
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    println!("NO_ROUTE {e:?}");
                    ExitCode::from(1)
                }
            }
        }
        [extract, out] => {
            let extract = PathBuf::from(extract);
            let out = PathBuf::from(out);
            let mut open = || {
                std::fs::File::open(&extract)
                    .map(|f| Box::new(std::io::BufReader::new(f)) as Box<dyn std::io::Read>)
                    .map_err(|e| format!("{}: {e}", extract.display()))
            };
            let mut write = |tile, bytes: &[u8]| write_to_folder(&out, tile, bytes);
            match generate(&mut open, &mut write) {
                Ok(g) => {
                    let tiles: Vec<String> =
                        g.tiles.iter().map(|(z, n)| format!("z{z}={n}")).collect();
                    println!("TILES {} ways {} nodes {}", g.ways, g.nodes, tiles.join(" "));
                    ExitCode::SUCCESS
                }
                Err(e) => {
                    eprintln!("{e}");
                    ExitCode::from(1)
                }
            }
        }
        _ => {
            eprintln!(
                "usage: routing-tiles <extract.osm.pbf> <out-dir>\n       routing-tiles --route \
                 <tile-dir> <lat,lon> <lat,lon> [car|bike|walk]"
            );
            ExitCode::from(2)
        }
    }
}
