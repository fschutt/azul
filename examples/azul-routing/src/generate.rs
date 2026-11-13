//! The tile generator: an OpenStreetMap extract cut into routing tiles, for
//! a region without a Valhalla build. It writes the same format a Valhalla
//! build gives, so the router reads both the same way; its tiles only lack
//! what Valhalla adds (turn restrictions, barriers, stop impacts, country
//! rules, road density).
//!
//! Two passes over the extract: the first keeps the routable ways (a
//! `highway=*` routing knows) and notes their nodes, the second reads those
//! nodes' positions. Then the ways are flattened into a graph ([`flatten`]):
//! a node where ways meet or end, an edge per direction of each stretch
//! between two such nodes, with Valhalla's road class, use and access mask,
//! the nodes between kept as the stretch's shape. [`crate::build::cut`]
//! cuts the graph into the tiles of zoom 8, 11 and 14.

use std::{
    collections::{BTreeMap, HashMap, HashSet},
    io::Read,
    path::Path,
};

use crate::{
    build::{cut, FlatEdge, FlatGraph, FlatNode, FlatShape},
    distance_m,
    format::{deg_to_e7, encode, flag, gzip, valhalla, Edge, Tile},
    manifest::{fill, LevelStats, Manifest, Region, DEFAULT_TILES, ODBL, ODBL_URL, OSM_ATTRIBUTION, OSM_COPYRIGHT_URL},
    offline::write_index,
    pbf::{read_blocks, OsmWay},
    source::KeepPolicy,
    tile::{TileId, LEVELS},
};

/// The kind of road (`highway=*`), the ones routing knows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Highway {
    Motorway,
    MotorwayLink,
    Trunk,
    TrunkLink,
    Primary,
    PrimaryLink,
    Secondary,
    SecondaryLink,
    Tertiary,
    TertiaryLink,
    Unclassified,
    Residential,
    LivingStreet,
    Service,
    Road,
    Track,
    Path,
    Footway,
    Cycleway,
    Bridleway,
    Pedestrian,
    Steps,
}

impl Highway {
    const ALL: [(Self, &'static str); 22] = [
        (Self::Motorway, "motorway"),
        (Self::MotorwayLink, "motorway_link"),
        (Self::Trunk, "trunk"),
        (Self::TrunkLink, "trunk_link"),
        (Self::Primary, "primary"),
        (Self::PrimaryLink, "primary_link"),
        (Self::Secondary, "secondary"),
        (Self::SecondaryLink, "secondary_link"),
        (Self::Tertiary, "tertiary"),
        (Self::TertiaryLink, "tertiary_link"),
        (Self::Unclassified, "unclassified"),
        (Self::Residential, "residential"),
        (Self::LivingStreet, "living_street"),
        (Self::Service, "service"),
        (Self::Road, "road"),
        (Self::Track, "track"),
        (Self::Path, "path"),
        (Self::Footway, "footway"),
        (Self::Cycleway, "cycleway"),
        (Self::Bridleway, "bridleway"),
        (Self::Pedestrian, "pedestrian"),
        (Self::Steps, "steps"),
    ];

    /// The OpenStreetMap value, `motorway_link`.
    #[must_use]
    pub fn key(self) -> &'static str {
        Self::ALL.iter().find(|(h, _)| *h == self).map_or("road", |(_, k)| k)
    }

    /// The kind a `highway` value names; `None` for a way that is not a road.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL.iter().find(|(_, k)| *k == value).map(|(h, _)| *h)
    }

    /// Valhalla's road classification: 0 motorway, 1 trunk, 2 primary, 3
    /// secondary, 4 tertiary, 5 unclassified, 6 residential, 7 service and
    /// other. A link has the class of the road it links. It decides the
    /// zooms a road is on ([`crate::tile::zoom_of_class`]).
    #[must_use]
    pub const fn class(self) -> u8 {
        match self {
            Self::Motorway | Self::MotorwayLink => 0,
            Self::Trunk | Self::TrunkLink => 1,
            Self::Primary | Self::PrimaryLink => 2,
            Self::Secondary | Self::SecondaryLink => 3,
            Self::Tertiary | Self::TertiaryLink => 4,
            Self::Unclassified => 5,
            Self::Residential | Self::LivingStreet | Self::Road => 6,
            _ => 7,
        }
    }

    /// Valhalla's use of the road ([`valhalla`]): a ramp for a link, a
    /// track, a footway, ... and a plain road for the rest.
    #[must_use]
    pub const fn use_(self) -> u8 {
        match self {
            Self::MotorwayLink | Self::TrunkLink | Self::PrimaryLink | Self::SecondaryLink | Self::TertiaryLink => {
                valhalla::USE_RAMP
            }
            Self::Track => valhalla::USE_TRACK,
            Self::LivingStreet => valhalla::USE_LIVING_STREET,
            Self::Service => valhalla::USE_SERVICE_ROAD,
            Self::Cycleway => valhalla::USE_CYCLEWAY,
            Self::Footway => valhalla::USE_FOOTWAY,
            Self::Steps => valhalla::USE_STEPS,
            Self::Path => valhalla::USE_PATH,
            Self::Pedestrian => valhalla::USE_PEDESTRIAN,
            Self::Bridleway => valhalla::USE_BRIDLEWAY,
            _ => 0,
        }
    }

    /// A car's usual speed on the road where it has no speed limit, km/h.
    #[must_use]
    pub const fn usual_kmh(self) -> f32 {
        match self {
            Self::Motorway => 115.0,
            Self::MotorwayLink => 60.0,
            Self::Trunk => 90.0,
            Self::TrunkLink => 50.0,
            Self::Primary => 70.0,
            Self::PrimaryLink => 45.0,
            Self::Secondary => 60.0,
            Self::SecondaryLink => 40.0,
            Self::Tertiary => 50.0,
            Self::TertiaryLink => 35.0,
            Self::Unclassified => 45.0,
            Self::Residential | Self::Road => 30.0,
            Self::LivingStreet => 10.0,
            Self::Service => 15.0,
            _ => 10.0,
        }
    }
}

/// Which way traffic may go along a way's nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Oneway {
    /// Both ways.
    #[default]
    No,
    /// In the order of the nodes only.
    Forward,
    /// Against it only (`oneway=-1`).
    Backward,
}

impl Oneway {
    /// Whether traffic may go along the nodes (`forward`) or against them.
    #[must_use]
    pub const fn allows(self, forward: bool) -> bool {
        match self {
            Self::No => true,
            Self::Forward => forward,
            Self::Backward => !forward,
        }
    }
}

/// Who may use a way, from `access` and its per-mode refinements; `None`
/// where the way says nothing (the road kind decides).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Access {
    /// Cars (`motor_vehicle`, `motorcar`, else `access`).
    pub motor: Option<bool>,
    /// Bicycles (`bicycle`, else `access`).
    pub bicycle: Option<bool>,
    /// Walkers (`foot`, else `access`).
    pub foot: Option<bool>,
}

/// One routable way of the extract.
#[derive(Debug, Clone, PartialEq)]
pub struct Way {
    /// The OpenStreetMap way id: it goes into the keys of the way's edges.
    pub id: i64,
    /// Its nodes' OpenStreetMap ids, in order.
    pub nodes: Vec<i64>,
    /// What kind of road it is.
    pub highway: Highway,
    /// Which way traffic may go.
    pub oneway: Oneway,
    /// Bicycles may ride against a car one-way (`oneway:bicycle=no`).
    pub bicycle_both_ways: bool,
    /// `maxspeed` in km/h, when it gives one.
    pub maxspeed_kmh: Option<f32>,
    /// Who may use it, where the tags say.
    pub access: Access,
    /// The road's name, for the route's list of streets.
    pub name: Option<String>,
}

impl Way {
    /// The Valhalla access mask of the way in one direction (`forward`
    /// along its nodes): cars and bicycles keep to one-way streets (a
    /// bicycle not where `oneway:bicycle=no`), walkers go both ways.
    #[must_use]
    pub fn access_mask(&self, forward: bool) -> u16 {
        use Highway::{Bridleway, Cycleway, Footway, Motorway, MotorwayLink, Path, Pedestrian, Steps, Track, Trunk, TrunkLink};
        let h = self.highway;
        let car = self.access.motor.unwrap_or(true)
            && !matches!(h, Track | Path | Footway | Cycleway | Bridleway | Pedestrian | Steps);
        let bike = self.access.bicycle.unwrap_or(!matches!(
            h,
            Motorway | MotorwayLink | Trunk | TrunkLink | Steps | Footway | Pedestrian | Bridleway
        ));
        let foot = self.access.foot.unwrap_or(!matches!(h, Motorway | MotorwayLink | Trunk | TrunkLink));
        let mut mask = 0;
        if car && self.oneway.allows(forward) {
            mask |= valhalla::AUTO;
        }
        if bike && (self.bicycle_both_ways || self.oneway.allows(forward)) {
            mask |= valhalla::BICYCLE;
        }
        if foot {
            mask |= valhalla::PEDESTRIAN;
        }
        mask
    }

    /// A car's speed on the way, km/h: a little under its speed limit, else
    /// the usual speed for its kind.
    #[must_use]
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // a speed in km/h
    pub fn car_kmh(&self) -> u32 {
        let kmh = self.maxspeed_kmh.map_or(self.highway.usual_kmh(), |limit| limit * 0.9);
        kmh.round().clamp(5.0, 130.0) as u32
    }
}

/// `maxspeed` in km/h: `50`, `30 mph`, the country words for town and land
/// (`DE:urban`), `walk`, `none` (no limit: the car's top speed).
#[must_use]
pub fn parse_maxspeed(value: &str) -> Option<f32> {
    let v = value.trim().to_ascii_lowercase();
    if let Some(mph) = v.strip_suffix("mph") {
        return mph.trim().parse::<f32>().ok().map(|s| s * 1.609_344);
    }
    if let Ok(kmh) = v.trim_end_matches("km/h").trim().parse::<f32>() {
        return (kmh > 0.0).then_some(kmh);
    }
    match v.rsplit(':').next()? {
        "urban" => Some(50.0),
        "rural" => Some(100.0),
        "motorway" => Some(130.0),
        "trunk" => Some(100.0),
        "living_street" | "walk" => Some(7.0),
        "zone30" | "zone:30" => Some(30.0),
        "none" | "signals" => Some(130.0),
        _ => None,
    }
}

/// A routing way read from its OpenStreetMap tags; `None` for a way routing
/// does not use (not a road, an area).
#[must_use]
pub fn way_from_tags(id: i64, nodes: Vec<i64>, tags: &[(String, String)]) -> Option<Way> {
    let tag = |key: &str| tags.iter().find(|(k, _)| k == key).map(|(_, v)| v.as_str());
    let highway = Highway::parse(tag("highway")?)?;
    if tag("area") == Some("yes") || nodes.len() < 2 {
        return None;
    }
    let allowed = |value: Option<&str>| match value? {
        "yes" | "designated" | "permissive" | "destination" => Some(true),
        "no" | "private" | "dismount" | "use_sidepath" => Some(false),
        _ => None,
    };
    let roundabout = matches!(tag("junction"), Some("roundabout" | "circular"));
    let oneway = match tag("oneway") {
        Some("yes" | "true" | "1") => Oneway::Forward,
        Some("-1" | "reverse") => Oneway::Backward,
        Some("no" | "false" | "0") => Oneway::No,
        _ if roundabout || highway == Highway::Motorway => Oneway::Forward,
        _ => Oneway::No,
    };
    let access = allowed(tag("access"));
    Some(Way {
        id,
        nodes,
        highway,
        oneway,
        bicycle_both_ways: tag("oneway:bicycle") == Some("no"),
        maxspeed_kmh: tag("maxspeed").and_then(parse_maxspeed),
        access: Access {
            motor: allowed(tag("motor_vehicle")).or_else(|| allowed(tag("motorcar"))).or(access),
            bicycle: allowed(tag("bicycle")).or(access),
            foot: allowed(tag("foot")).or(access),
        },
        name: tag("name").map(str::to_string),
    })
}

/// The flat graph of `ways` (node positions from `coords`): a node where
/// ways meet or end, and for each stretch of a way between two such nodes an
/// edge per direction, with the nodes between kept as the stretch's shape.
/// A node without a position cuts its way; a closed way is also cut at its
/// middle, so no edge starts and ends at one node.
#[must_use]
pub fn flatten(ways: &[Way], coords: &HashMap<i64, (f64, f64)>) -> FlatGraph {
    // A node two ways share, or one way passes twice, is where they meet.
    let mut uses: HashMap<i64, u32> = HashMap::new();
    for way in ways {
        for node in &way.nodes {
            *uses.entry(*node).or_default() += 1;
        }
    }
    let mut flat = Flattening { graph: FlatGraph::default(), index: HashMap::new(), places: HashMap::new(), pairs: Vec::new() };
    for way in ways {
        for run in way.nodes.split(|n| !coords.contains_key(n)) {
            if run.len() < 2 {
                continue;
            }
            let last = run.len() - 1;
            let middle = (run[0] == run[last]).then_some(last / 2);
            let mut start = 0;
            for i in 1..=last {
                if i == last || uses.get(&run[i]).is_some_and(|&n| n > 1) || Some(i) == middle {
                    if run[start] != run[i] {
                        flat.stretch(way, &run[start..=i], coords);
                    }
                    start = i;
                }
            }
        }
    }
    flat.number();
    flat.graph
}

/// The graph [`flatten`] builds, and what it needs while it builds it.
struct Flattening {
    /// The graph so far.
    graph: FlatGraph,
    /// The graph node of each OpenStreetMap node that is one.
    index: HashMap<i64, usize>,
    /// The OpenStreetMap node first seen at each place (1e-7 degrees): a
    /// second node at the very same place is its twin, so the keys differ.
    places: HashMap<(i32, i32), i64>,
    /// The two edges of each stretch, there and back.
    pairs: Vec<(usize, usize)>,
}

impl Flattening {
    /// The graph node of OpenStreetMap node `id` at `at`, added when new.
    fn node(&mut self, id: i64, at: (f64, f64)) -> usize {
        if let Some(&index) = self.index.get(&id) {
            return index;
        }
        let (lat, lon) = (deg_to_e7(at.0), deg_to_e7(at.1));
        let twin = *self.places.entry((lat, lon)).or_insert(id) != id;
        self.graph.nodes.push(FlatNode { lat, lon, twin, access: valhalla::ALL, kind: 0, info: 0 });
        self.index.insert(id, self.graph.nodes.len() - 1);
        self.graph.nodes.len() - 1
    }

    /// Adds the stretch `nodes` of `way` (its ends are graph nodes) as an
    /// edge there and an edge back.
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // metres and OpenStreetMap ids
    fn stretch(&mut self, way: &Way, nodes: &[i64], coords: &HashMap<i64, (f64, f64)>) {
        let points: Vec<(f64, f64)> = nodes.iter().map(|n| coords[n]).collect();
        let (first, last) = (nodes[0], nodes[nodes.len() - 1]);
        let a = self.node(first, points[0]);
        let b = self.node(last, points[points.len() - 1]);
        let e6 = |v: f64| (v * 1e6).round() as i32;
        self.graph.shapes.push(FlatShape {
            way_id: way.id as u64,
            name: way.name.clone(),
            points: points[1..points.len() - 1].iter().map(|&(lat, lon)| (e6(lat), e6(lon))).collect(),
        });
        let shape = self.graph.shapes.len() - 1;
        let length: f64 = points.windows(2).map(|w| distance_m(w[0], w[1])).sum();
        let link = way.highway.use_() == valhalla::USE_RAMP;
        let data = |access: u16| Edge {
            from: 0,
            to: 0,
            length_m: length.round() as u32,
            speed_kmh: way.car_kmh(),
            access,
            class: way.highway.class(),
            use_: way.highway.use_(),
            density: 0,
            flags: if link { flag::LINK } else { 0 },
            local_index: 0,
            opp_local_index: 0,
            restrictions: 0,
            shape: 0,
            reversed: false,
            stop_impact: 0,
            same_name: 0,
        };
        self.graph.edges.push(FlatEdge { from: a, to: b, shape, reversed: false, data: data(way.access_mask(true)) });
        self.graph.edges.push(FlatEdge { from: b, to: a, shape, reversed: true, data: data(way.access_mask(false)) });
        self.pairs.push((self.graph.edges.len() - 2, self.graph.edges.len() - 1));
    }

    /// Numbers the edges leaving each node (their local index), tells each
    /// edge the local index of its way back, and marks which edges at a
    /// node share a street name, as Valhalla's graph has them: the router's
    /// turn restrictions, U-turns and street changes read these.
    fn number(&mut self) {
        let edges = &mut self.graph.edges;
        let mut leaving: Vec<Vec<usize>> = vec![Vec::new(); self.graph.nodes.len()];
        for (index, edge) in edges.iter().enumerate() {
            leaving[edge.from].push(index);
        }
        for at_node in &leaving {
            for (local, &e) in at_node.iter().enumerate() {
                edges[e].data.local_index = u8::try_from(local).unwrap_or(127).min(127);
            }
        }
        for &(there, back) in &self.pairs {
            edges[there].data.opp_local_index = edges[back].data.local_index;
            edges[back].data.opp_local_index = edges[there].data.local_index;
        }
        let shapes = &self.graph.shapes;
        let names: Vec<Option<&str>> = edges.iter().map(|e| shapes[e.shape].name.as_deref()).collect();
        for at_node in &leaving {
            for &e in at_node {
                let mut same = 0u8;
                if let Some(own) = names[e] {
                    for &other in at_node {
                        let i = edges[other].data.local_index;
                        if other != e && i < 8 && names[other] == Some(own) {
                            same |= 1 << i;
                        }
                    }
                }
                edges[e].data.same_name = same;
            }
        }
    }
}

/// What the generator wrote.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Generated {
    /// The routable ways it found.
    pub ways: usize,
    /// The nodes of the graph: where ways meet or end.
    pub nodes: usize,
    /// The edges of the graph, one per direction of each stretch.
    pub edges: usize,
    /// The tiles written per zoom, `(zoom, count)`, coarse to fine.
    pub tiles: Vec<(u8, usize)>,
    /// Every tile written and its size as served: the tile list.
    pub index: BTreeMap<TileId, u64>,
    /// `[west, south, east, north]` of the graph's nodes, 1e-7 degrees.
    pub bbox_e7: [i32; 4],
}

/// Cuts the routable ways of an extract into tiles: `open()` opens the
/// extract (it is read twice), `write(tile, bytes)` stores one gzipped tile.
///
/// # Errors
/// The extract does not read ([`read_blocks`]), or `write` fails.
pub fn generate(
    open: &mut dyn FnMut() -> Result<Box<dyn Read>, String>,
    write: &mut dyn FnMut(TileId, &[u8]) -> Result<(), String>,
) -> Result<Generated, String> {
    let mut ways: Vec<Way> = Vec::new();
    let mut wanted: HashSet<i64> = HashSet::new();
    read_blocks(&mut *open()?, true, &mut |block| {
        for OsmWay { id, nodes, tags } in block.ways {
            if let Some(way) = way_from_tags(id, nodes, &tags) {
                wanted.extend(way.nodes.iter().copied());
                ways.push(way);
            }
        }
        Ok(())
    })?;
    let mut coords: HashMap<i64, (f64, f64)> = HashMap::with_capacity(wanted.len());
    read_blocks(&mut *open()?, false, &mut |block| {
        for (id, lat, lon) in block.nodes {
            if wanted.contains(&id) {
                coords.insert(id, (lat, lon));
            }
        }
        Ok(())
    })?;
    let graph = flatten(&ways, &coords);
    let mut generated = Generated {
        ways: ways.len(),
        nodes: graph.nodes.len(),
        edges: graph.edges.len(),
        tiles: Vec::new(),
        index: BTreeMap::new(),
        bbox_e7: bbox_e7(&graph),
    };
    for z in LEVELS {
        let tiles = cut(&graph, z);
        generated.tiles.push((z, tiles.len()));
        for (tile, data) in tiles {
            let bytes = gzip(&encode(&data));
            generated.index.insert(tile, bytes.len() as u64);
            write(tile, &bytes)?;
        }
    }
    Ok(generated)
}

/// `[west, south, east, north]` of the graph's nodes, 1e-7 degrees.
fn bbox_e7(graph: &FlatGraph) -> [i32; 4] {
    let mut bbox = [i32::MAX, i32::MAX, i32::MIN, i32::MIN];
    for node in &graph.nodes {
        bbox = [bbox[0].min(node.lon), bbox[1].min(node.lat), bbox[2].max(node.lon), bbox[3].max(node.lat)];
    }
    if graph.nodes.is_empty() {
        [0; 4]
    } else {
        bbox
    }
}

/// The tiles of zoom `z` of `ways`: [`flatten`] and [`cut`] in one, for a
/// handful of ways.
#[must_use]
pub fn cut_level(ways: &[Way], coords: &HashMap<i64, (f64, f64)>, z: u8) -> BTreeMap<TileId, Tile> {
    cut(&flatten(ways, coords), z)
}

/// Writes tile `tile` under `root` where a manifest's default template puts
/// it: `<root>/tiles/<z>/<x>/<y>.pbf`.
///
/// # Errors
/// The folder or the file cannot be written.
pub fn write_to_folder(root: &Path, tile: TileId, bytes: &[u8]) -> Result<(), String> {
    let path = root.join(fill(DEFAULT_TILES, tile));
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    }
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))
}

/// The manifest of the tiles `generated` describes, built from the extract
/// `source` (its file name or URL, nothing about the machine it is on) of
/// region `region` at `built_at` (RFC 3339).
#[must_use]
pub fn manifest_of(generated: &Generated, region: &str, source: &str, built_at: &str) -> Manifest {
    let b = generated.bbox_e7;
    let bbox = [f64::from(b[0]) * 1e-7, f64::from(b[1]) * 1e-7, f64::from(b[2]) * 1e-7, f64::from(b[3]) * 1e-7];
    let day = built_at.get(..10).unwrap_or(built_at);
    Manifest {
        version: crate::format::VERSION,
        build: format!("{day}-{region}"),
        tiles: DEFAULT_TILES.to_string(),
        index: Some("tiles.txt".to_string()),
        bbox,
        regions: vec![Region { id: region.to_string(), source: source.to_string(), osm_timestamp: String::new(), bbox }],
        built_at: built_at.to_string(),
        valhalla: String::new(),
        valhalla_image: String::new(),
        attribution: OSM_ATTRIBUTION.to_string(),
        attribution_url: OSM_COPYRIGHT_URL.to_string(),
        license: ODBL.to_string(),
        license_url: ODBL_URL.to_string(),
        keep_days: KeepPolicy::default().days,
        levels: LEVELS
            .iter()
            .map(|&z| {
                let (tiles, bytes) = generated
                    .index
                    .iter()
                    .filter(|(t, _)| t.z == z)
                    .fold((0, 0), |(tiles, sum), (_, bytes)| (tiles + 1, sum + bytes));
                LevelStats { z, tiles, bytes }
            })
            .collect(),
    }
}

/// Writes the manifest ([`manifest_of`]) and the tile list of the tiles
/// written under `root`, next to them; returns the manifest.
///
/// # Errors
/// A file cannot be written.
pub fn write_manifest(root: &Path, generated: &Generated, region: &str, source: &str, built_at: &str) -> Result<Manifest, String> {
    let manifest = manifest_of(generated, region, source, built_at);
    std::fs::create_dir_all(root).map_err(|e| format!("{}: {e}", root.display()))?;
    for (name, text) in [("manifest.json", manifest.to_json()), ("tiles.txt", write_index(&generated.index))] {
        let path = root.join(name);
        std::fs::write(&path, text).map_err(|e| format!("{}: {e}", path.display()))?;
    }
    Ok(manifest)
}

/// A time in seconds since 1970 as RFC 3339 (`2026-10-14T20:46:34Z`), the
/// form the manifest keeps its build time in.
#[must_use]
#[allow(clippy::cast_possible_wrap)] // seconds since 1970 fit an i64 for ages
pub fn rfc3339(unix_seconds: u64) -> String {
    // The civil date of a day number (Howard Hinnant's civil_from_days).
    let days = (unix_seconds / 86_400) as i64 + 719_468;
    let era = days.div_euclid(146_097);
    let day_of_era = days.rem_euclid(146_097);
    let year_of_era = (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let shifted_month = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * shifted_month + 2) / 5 + 1;
    let month = if shifted_month < 10 { shifted_month + 3 } else { shifted_month - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    let seconds = unix_seconds % 86_400;
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds / 3600,
        seconds % 3600 / 60,
        seconds % 60
    )
}
