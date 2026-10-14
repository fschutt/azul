//! `routing-tiles <extract.osm.pbf> <out-dir>`: cuts an OpenStreetMap
//! extract into AzMaps' routing tiles where no Valhalla build exists:
//! `<out-dir>/tiles/<z>/<x>/<y>.pbf` for zoom 8, 11 and 14, the tile list
//! `tiles.txt` and `manifest.json`, ready for any static file hosting.
//! `--routing <out-dir>/manifest.json` (or its URL) routes on them.
//!
//! `routing-tiles --route <manifest|dir> <lat,lon> <lat,lon> [car|bike|walk]`
//! works out a route on such a folder (or a Valhalla build's) and prints
//! it, for a quick check of the tiles without the app.

use std::{path::PathBuf, process::ExitCode};

use azul_routing::{
    generate::{generate, rfc3339, write_manifest, write_to_folder},
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
        [flag, root, from, to, rest @ ..] if flag == "--route" => {
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
            let source = FolderTiles { root: PathBuf::from(root) };
            match route(&source, from, to, mode, &mut |_| {}, &mut || false) {
                Ok(r) => {
                    println!(
                        "ROUTE {:.3} km {:.1} min {} points {} tiles {} expanded {} attempts {} edges",
                        r.distance_m / 1000.0,
                        r.duration_s / 60.0,
                        r.points.len(),
                        r.tiles,
                        r.expanded,
                        r.attempts,
                        r.edges.len()
                    );
                    if let Ok(manifest) = source.manifest() {
                        println!("DATA {} ({})", manifest.attribution, manifest.attribution_url);
                    }
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
            let generated = match generate(&mut open, &mut write) {
                Ok(g) => g,
                Err(e) => {
                    eprintln!("{e}");
                    return ExitCode::from(1);
                }
            };
            // The manifest names the extract by its file name only: nothing
            // about the machine it was built on.
            let file = extract.file_name().map_or_else(String::new, |f| f.to_string_lossy().into_owned());
            let region = file.split('.').next().unwrap_or("region").trim_end_matches("-latest").to_string();
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map_or(0, |d| d.as_secs());
            if let Err(e) = write_manifest(&out, &generated, &region, &file, &rfc3339(now)) {
                eprintln!("{e}");
                return ExitCode::from(1);
            }
            let tiles: Vec<String> = generated.tiles.iter().map(|(z, n)| format!("z{z}={n}")).collect();
            println!(
                "TILES {} ways {} nodes {} {} edges",
                generated.ways,
                generated.nodes,
                tiles.join(" "),
                generated.edges
            );
            ExitCode::SUCCESS
        }
        _ => {
            eprintln!(
                "usage: routing-tiles <extract.osm.pbf> <out-dir>\n       routing-tiles --route \
                 <manifest|dir> <lat,lon> <lat,lon> [car|bike|walk]"
            );
            ExitCode::from(2)
        }
    }
}
