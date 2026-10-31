//! One routing tile in memory, and its protobuf form: [`decode`] reads a
//! tile as it is served (gzipped or not), [`encode`] writes one.
//!
//! The tiles are Valhalla's road graph cut into the map's XYZ tiles at zoom
//! 8, 11 and 14 (azul-routing-tiles builds them; its
//! `schema/routing_tile.proto` is the schema). The structs mirror the
//! schema's columns row by row, so reading is a straight copy and nothing is
//! lost between [`encode`] and [`decode`].

use std::io::Read;

use crate::{
    tile::TileId,
    wire::{read_fixed64s, read_varints, undelta, Reader, Value, Writer},
};

/// The format version this crate reads and writes.
pub const VERSION: u32 = 1;

/// Valhalla's values, as the tiles keep them (`valhalla/baldr/graphconstants.h`).
pub mod valhalla {
    /// Access bits: who may use an edge or pass a node.
    pub const AUTO: u16 = 1;
    pub const PEDESTRIAN: u16 = 2;
    pub const BICYCLE: u16 = 4;
    pub const TRUCK: u16 = 8;
    pub const ALL: u16 = 4095;

    /// Edge uses: the ones the router treats specially, and the ones the
    /// generator gives OpenStreetMap's paths.
    pub const USE_RAMP: u8 = 1;
    pub const USE_TRACK: u8 = 3;
    pub const USE_LIVING_STREET: u8 = 10;
    pub const USE_SERVICE_ROAD: u8 = 11;
    pub const USE_CYCLEWAY: u8 = 20;
    pub const USE_FOOTWAY: u8 = 25;
    pub const USE_STEPS: u8 = 26;
    pub const USE_PATH: u8 = 27;
    pub const USE_PEDESTRIAN: u8 = 28;
    pub const USE_BRIDLEWAY: u8 = 29;
    pub const USE_FERRY: u8 = 41;
    pub const USE_RAIL_FERRY: u8 = 42;
    pub const USE_CONSTRUCTION: u8 = 43;
    /// From here on the uses belong to public transport, not to roads.
    pub const USE_FIRST_TRANSIT: u8 = 50;

    /// Node kinds (`NodeType`) that cost time to pass.
    pub const NODE_GATE: u8 = 1;
    pub const NODE_BOLLARD: u8 = 2;
    pub const NODE_TOLL_BOOTH: u8 = 3;
    pub const NODE_BORDER_CONTROL: u8 = 10;

    /// The worst surface (`Surface::kImpassable`).
    pub const SURFACE_IMPASSABLE: u8 = 7;
}

/// The bits of [`Edge::flags`] (the schema's `EdgeFlag`).
pub mod flag {
    pub const TOLL: u32 = 1;
    pub const ROUNDABOUT: u32 = 1 << 1;
    pub const LINK: u32 = 1 << 2;
    pub const DEST_ONLY: u32 = 1 << 3;
    pub const NOT_THRU: u32 = 1 << 4;
    pub const TUNNEL: u32 = 1 << 5;
    pub const BRIDGE: u32 = 1 << 6;
    pub const INTERNAL: u32 = 1 << 7;
    pub const DEADEND: u32 = 1 << 8;
    pub const COUNTRY_CROSSING: u32 = 1 << 9;
    pub const TRAFFIC_SIGNAL: u32 = 1 << 10;
    pub const STOP_SIGN: u32 = 1 << 11;
    pub const YIELD_SIGN: u32 = 1 << 12;
    pub const DISMOUNT: u32 = 1 << 13;
    /// The surface (0 smooth paving .. 7 impassable) sits in bits 16-18.
    pub const SURFACE_SHIFT: u32 = 16;
    /// The hiking difficulty (`sac_scale`, 0 none .. 6) sits in bits 19-21.
    pub const SAC_SHIFT: u32 = 19;
}

/// A node of the graph.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Node {
    /// Latitude in 1e-7 degrees.
    pub lat: i32,
    /// Longitude in 1e-7 degrees.
    pub lon: i32,
    /// The second of two nodes at the very same place.
    pub twin: bool,
    /// See [`node_info`]: road density, driving side, traffic signal.
    pub info: u8,
}

/// The bits of [`Node::info`].
pub mod node_info {
    /// The density of roads around the node, 0-15, in the low four bits.
    pub const DENSITY_MASK: u8 = 15;
    /// Traffic keeps left here.
    pub const DRIVE_LEFT: u8 = 16;
    pub const TRAFFIC_SIGNAL: u8 = 32;
}

impl Node {
    /// The node's place as `(lat, lon)` degrees.
    #[must_use]
    pub fn latlon(&self) -> (f64, f64) {
        (e7_to_deg(self.lat), e7_to_deg(self.lon))
    }

    /// The node's key: its place, the same in every tile, zoom and build.
    #[must_use]
    pub fn key(&self) -> u64 {
        node_key(self.lat, self.lon, self.twin)
    }
}

/// The key of the node at `(lat, lon)` (1e-7 degrees); `twin` for the second
/// node at the same place.
#[must_use]
#[allow(clippy::cast_sign_loss)] // both sums are positive on Earth
pub fn node_key(lat: i32, lon: i32, twin: bool) -> u64 {
    let lat = (i64::from(lat) + 900_000_000) as u64;
    let lon = (i64::from(lon) + 1_800_000_000) as u64;
    (lat << 32) | lon | (u64::from(twin) << 63)
}

/// The key of the edge from node `from` to node `to` along way `way_id`:
/// 64-bit FNV-1a over the three values' little-endian bytes.
#[must_use]
pub fn edge_key(from: u64, to: u64, way_id: u64) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in from.to_le_bytes().into_iter().chain(to.to_le_bytes()).chain(way_id.to_le_bytes()) {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0100_0000_01b3);
    }
    hash
}

/// A node that is closed to some modes or costs time to pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Special {
    /// The node, by its index in [`Tile::nodes`].
    pub node: u32,
    /// Who may pass (Valhalla access mask).
    pub access: u16,
    /// What it is (Valhalla `NodeType`).
    pub kind: u8,
}

/// One direction of travel along a road between two nodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Edge {
    /// The start node, by its index in [`Tile::nodes`].
    pub from: u32,
    /// The end node, by its index in [`Tile::nodes`].
    pub to: u32,
    /// The length along the road, metres.
    pub length_m: u32,
    /// Valhalla's speed for cars on it, km/h (its speed limit, else a
    /// default for the road class, the area and the country).
    pub speed_kmh: u32,
    /// Who may travel along it in this direction (Valhalla access mask): a
    /// one-way street has no car bit on the edge against it.
    pub access: u16,
    /// Valhalla road classification: 0 motorway .. 7 service / other.
    pub class: u8,
    /// Valhalla use: road, ramp, footway, ferry, ... (see [`valhalla`]).
    pub use_: u8,
    /// The density of roads around the edge, 0-15.
    pub density: u8,
    /// See [`flag`].
    pub flags: u32,
    /// This edge's index among the edges leaving its start node.
    pub local_index: u8,
    /// The local index of the edge going back from the end node.
    pub opp_local_index: u8,
    /// Simple turn restrictions: bit i forbids going on along the end
    /// node's edge whose local index is i.
    pub restrictions: u8,
    /// The shape, by its index in [`Tile::shapes`].
    pub shape: u32,
    /// The edge runs against the shape's direction.
    pub reversed: bool,
    /// Valhalla's stop impact of turning into this edge, 3 bits per local
    /// index at the start node (see [`Edge::stop_from`]).
    pub stop_impact: u32,
    /// Bit i: this edge shares a name with the edge of local index i at its
    /// start node.
    pub same_name: u8,
}

impl Edge {
    /// The stop impact (0-7) of turning into this edge when arriving on the
    /// edge whose way back has local index `back_index` here.
    #[must_use]
    pub const fn stop_from(&self, back_index: u8) -> u8 {
        if back_index >= 8 {
            0
        } else {
            ((self.stop_impact >> (3 * back_index as u32)) & 7) as u8
        }
    }

    /// Whether this edge goes on the street of the edge whose way back has
    /// local index `back_index` at the start node.
    #[must_use]
    pub const fn same_street_as(&self, back_index: u8) -> bool {
        back_index < 8 && self.same_name & (1 << back_index) != 0
    }

    /// The surface, 0 (smooth paving) to 7 (impassable).
    #[must_use]
    pub const fn surface(&self) -> u8 {
        ((self.flags >> flag::SURFACE_SHIFT) & 7) as u8
    }

    /// The hiking difficulty, 0 (none) to 6.
    #[must_use]
    pub const fn sac_scale(&self) -> u8 {
        ((self.flags >> flag::SAC_SHIFT) & 7) as u8
    }

    /// Whether flag `bit` ([`flag`]) is set.
    #[must_use]
    pub const fn has(&self, bit: u32) -> bool {
        self.flags & bit != 0
    }
}

/// The geometry of one stretch of road, shared by both its directions.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Shape {
    /// The OpenStreetMap way it is part of.
    pub way_id: u64,
    /// Its street name, by index in [`Tile::names`].
    pub name: Option<u32>,
    /// The points between its two end nodes, `(lat, lon)` in 1e-6 degrees
    /// (nodes are 1e-7: their places are their keys).
    pub points: Vec<(i32, i32)>,
}

/// A turn restriction over several edges, or for some modes or times only.
/// Edges are named by their keys ([`Tile::edge_key`]).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Restriction {
    /// The edge it forbids going into.
    pub to_edge: u64,
    /// The edge the forbidden way comes from.
    pub from_edge: u64,
    /// The edges between, the one next to `to_edge` first.
    pub vias: Vec<u64>,
    /// The modes it applies to (Valhalla access mask).
    pub modes: u16,
    /// It holds at certain times only.
    pub timed: bool,
}

/// One routing tile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tile {
    /// Which tile this is.
    pub id: TileId,
    /// Every node an edge of the tile starts or ends at, also those just
    /// outside it, so each edge is complete. Sorted by key.
    pub nodes: Vec<Node>,
    /// The nodes closed to some modes or costing time to pass.
    pub specials: Vec<Special>,
    /// The directed edges, sorted by start node.
    pub edges: Vec<Edge>,
    /// The geometry of the edges, one per stretch of road.
    pub shapes: Vec<Shape>,
    /// The street names the shapes use.
    pub names: Vec<String>,
    /// The turn restrictions over several edges whose last edge is here.
    pub restrictions: Vec<Restriction>,
}

impl Tile {
    /// An empty tile `id`.
    #[must_use]
    pub const fn new(id: TileId) -> Self {
        Self {
            id,
            nodes: Vec::new(),
            specials: Vec::new(),
            edges: Vec::new(),
            shapes: Vec::new(),
            names: Vec::new(),
            restrictions: Vec::new(),
        }
    }

    /// The whole line of `edge`, `(lat, lon)` degrees, from its start node
    /// to its end node.
    #[must_use]
    pub fn edge_points(&self, edge: &Edge) -> Vec<(f64, f64)> {
        let mut points = vec![self.nodes[edge.from as usize].latlon()];
        if let Some(shape) = self.shapes.get(edge.shape as usize) {
            let middle = shape.points.iter().map(|&(lat, lon)| (f64::from(lat) * 1e-6, f64::from(lon) * 1e-6));
            if edge.reversed {
                points.extend(middle.rev());
            } else {
                points.extend(middle);
            }
        }
        points.push(self.nodes[edge.to as usize].latlon());
        points
    }

    /// The key of `edge`, the same in every tile, zoom and build.
    #[must_use]
    pub fn edge_key(&self, edge: &Edge) -> u64 {
        let way = self.shapes.get(edge.shape as usize).map_or(0, |s| s.way_id);
        edge_key(self.nodes[edge.from as usize].key(), self.nodes[edge.to as usize].key(), way)
    }

    /// The street name of `edge`, if it has one.
    #[must_use]
    pub fn edge_name(&self, edge: &Edge) -> Option<&str> {
        let shape = self.shapes.get(edge.shape as usize)?;
        self.names.get(shape.name? as usize).map(String::as_str)
    }
}

/// 1e-7 degrees to degrees.
#[must_use]
pub fn e7_to_deg(v: i32) -> f64 {
    f64::from(v) * 1e-7
}

/// Degrees to 1e-7 degrees, rounded.
#[must_use]
#[allow(clippy::cast_possible_truncation)] // a coordinate fits easily
pub fn deg_to_e7(v: f64) -> i32 {
    (v * 1e7).round() as i32
}

/// Writes `tile` as protobuf (not yet gzipped; see [`gzip`]).
#[must_use]
#[allow(clippy::cast_possible_wrap)] // shape ways are OpenStreetMap ids
pub fn encode(tile: &Tile) -> Vec<u8> {
    let mut w = Writer::default();
    w.uint(1, u64::from(VERSION));
    w.uint(2, u64::from(tile.id.z));
    w.uint(3, u64::from(tile.id.x));
    w.uint(4, u64::from(tile.id.y));
    w.delta(5, tile.nodes.iter().map(|n| i64::from(n.lat)));
    w.delta(6, tile.nodes.iter().map(|n| i64::from(n.lon)));
    w.delta(7, (0..tile.nodes.len()).filter(|&i| tile.nodes[i].twin).map(|i| i as i64));
    w.packed(26, tile.nodes.iter().map(|n| u64::from(n.info)));
    w.delta(8, tile.specials.iter().map(|s| i64::from(s.node)));
    w.packed(9, tile.specials.iter().map(|s| u64::from(s.access)));
    w.packed(10, tile.specials.iter().map(|s| u64::from(s.kind)));
    w.delta(11, tile.edges.iter().map(|e| i64::from(e.from)));
    w.packed(12, tile.edges.iter().map(|e| u64::from(e.to)));
    w.packed(13, tile.edges.iter().map(|e| u64::from(e.length_m)));
    w.packed(14, tile.edges.iter().map(|e| u64::from(e.speed_kmh)));
    w.packed(15, tile.edges.iter().map(|e| u64::from(e.access)));
    w.packed(
        16,
        tile.edges
            .iter()
            .map(|e| u64::from(e.class & 7) | u64::from(e.use_ & 63) << 3 | u64::from(e.density & 15) << 9),
    );
    w.packed(17, tile.edges.iter().map(|e| u64::from(e.flags)));
    w.packed(
        18,
        tile.edges.iter().map(|e| {
            u64::from(e.local_index & 127)
                | u64::from(e.opp_local_index & 127) << 7
                | u64::from(e.restrictions) << 14
        }),
    );
    w.packed(19, tile.edges.iter().map(|e| u64::from(e.shape) << 1 | u64::from(e.reversed)));
    w.packed(27, tile.edges.iter().map(|e| u64::from(e.stop_impact)));
    w.packed(28, tile.edges.iter().map(|e| u64::from(e.same_name)));
    w.delta(20, tile.shapes.iter().map(|s| s.way_id as i64));
    w.packed(21, tile.shapes.iter().map(|s| s.name.map_or(0, |n| u64::from(n) + 1)));
    w.packed(22, tile.shapes.iter().map(|s| s.points.len() as u64));
    w.delta(
        23,
        tile.shapes
            .iter()
            .flat_map(|s| s.points.iter().flat_map(|&(lat, lon)| [i64::from(lat), i64::from(lon)])),
    );
    for name in &tile.names {
        w.bytes(24, name.as_bytes());
    }
    for r in &tile.restrictions {
        let mut m = Writer::default();
        m.fixed64(1, r.to_edge);
        m.fixed64(2, r.from_edge);
        m.packed_fixed64(3, &r.vias);
        m.uint(4, u64::from(r.modes));
        m.uint(5, u64::from(r.timed));
        w.bytes(25, &m.buf);
    }
    w.buf
}

/// `bytes` gzipped, as tiles are served.
#[must_use]
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::best());
    // Writing into a Vec only fails when memory runs out.
    encoder.write_all(bytes).expect("gzip into memory");
    encoder.finish().expect("gzip into memory")
}

/// The raw columns of a tile as they come off the wire.
#[derive(Default)]
struct Columns {
    /// The format version the tile says it has.
    version: u64,
    /// The tile's z, x and y.
    zxy: [u64; 3],
    /// The numeric columns by field number (5 to 28), as raw varints.
    u: [Vec<u64>; 29],
    /// The street names, in order.
    names: Vec<String>,
    /// The restrictions over several edges.
    restrictions: Vec<Restriction>,
}

/// Reads one tile, gzipped or not.
///
/// # Errors
/// The bytes are not a tile of a version this crate reads, or its columns
/// do not fit together (different lengths, an index out of range).
pub fn decode(bytes: &[u8]) -> Result<Tile, String> {
    let unpacked;
    let bytes = if bytes.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut out)
            .map_err(|e| format!("gzip: {e}"))?;
        unpacked = out;
        &unpacked[..]
    } else {
        bytes
    };
    let mut c = Columns::default();
    let mut r = Reader::new(bytes);
    while let Some((number, value)) = r.field()? {
        match (number, value) {
            (1, Value::Varint(v)) => c.version = v,
            (2..=4, Value::Varint(v)) => c.zxy[number as usize - 2] = v,
            (5..=23 | 26..=28, value) => read_varints(value, &mut c.u[number as usize])?,
            (24, Value::Bytes(b)) => c.names.push(
                String::from_utf8(b.to_vec()).map_err(|_| "a name is not UTF-8".to_string())?,
            ),
            (25, Value::Bytes(b)) => c.restrictions.push(read_restriction(b)?),
            // A field this version does not know: a later, compatible one.
            _ => {}
        }
    }
    if c.version != u64::from(VERSION) {
        return Err(format!("tile format version {} (this reader knows {VERSION})", c.version));
    }
    assemble(c)
}

fn read_restriction(bytes: &[u8]) -> Result<Restriction, String> {
    let mut r = Reader::new(bytes);
    let mut out = Restriction {
        to_edge: 0,
        from_edge: 0,
        vias: Vec::new(),
        modes: 0,
        timed: false,
    };
    while let Some((number, value)) = r.field()? {
        match (number, value) {
            (1, Value::Fixed64(v)) => out.to_edge = v,
            (2, Value::Fixed64(v)) => out.from_edge = v,
            (3, value) => read_fixed64s(value, &mut out.vias)?,
            #[allow(clippy::cast_possible_truncation)]
            (4, Value::Varint(v)) => out.modes = v as u16,
            (5, Value::Varint(v)) => out.timed = v != 0,
            _ => {}
        }
    }
    Ok(out)
}

#[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss, clippy::too_many_lines)]
fn assemble(c: Columns) -> Result<Tile, String> {
    let u = &c.u;
    let same = |what: &str, first: usize, others: &[usize]| {
        if others.iter().all(|&n| u[n].len() == u[first].len()) {
            Ok(u[first].len())
        } else {
            Err(format!("the {what} columns have different lengths"))
        }
    };
    let z = u8::try_from(c.zxy[0]).map_err(|_| "z is too big")?;
    let id = TileId::new(z, c.zxy[1] as u32, c.zxy[2] as u32);
    let mut tile = Tile::new(id);

    let node_count = same("node", 5, &[6, 26])?;
    let (lats, lons) = (undelta(&u[5]), undelta(&u[6]));
    tile.nodes = (0..node_count)
        .map(|i| Node {
            lat: lats[i] as i32,
            lon: lons[i] as i32,
            twin: false,
            info: u[26][i] as u8,
        })
        .collect();
    let index = |v: i64, len: usize, what: &str| {
        u32::try_from(v)
            .ok()
            .filter(|&i| (i as usize) < len)
            .ok_or_else(|| format!("{what} {v} is out of range"))
    };
    for twin in undelta(&u[7]) {
        tile.nodes[index(twin, node_count, "twin node")? as usize].twin = true;
    }

    let special_count = same("special node", 8, &[9, 10])?;
    let special_nodes = undelta(&u[8]);
    for i in 0..special_count {
        tile.specials.push(Special {
            node: index(special_nodes[i], node_count, "special node")?,
            access: u[9][i] as u16,
            kind: u[10][i] as u8,
        });
    }

    let shape_count = same("shape", 20, &[21, 22])?;
    let ways = undelta(&u[20]);
    let coords = undelta(&u[23]);
    let mut at = 0usize;
    for i in 0..shape_count {
        let points = u[22][i] as usize;
        let end = at
            .checked_add(points * 2)
            .filter(|&end| end <= coords.len())
            .ok_or("the shape points run past the coordinates")?;
        let name = match u[21][i] {
            0 => None,
            n => Some(index(n as i64 - 1, c.names.len(), "name")?),
        };
        tile.shapes.push(Shape {
            way_id: ways[i] as u64,
            name,
            points: coords[at..end]
                .chunks_exact(2)
                .map(|p| (p[0] as i32, p[1] as i32))
                .collect(),
        });
        at = end;
    }

    let edge_count = same("edge", 11, &[12, 13, 14, 15, 16, 17, 18, 19, 27, 28])?;
    let froms = undelta(&u[11]);
    for i in 0..edge_count {
        let (class, turn, shape) = (u[16][i], u[18][i], u[19][i]);
        tile.edges.push(Edge {
            from: index(froms[i], node_count, "edge start")?,
            to: index(u[12][i] as i64, node_count, "edge end")?,
            length_m: u[13][i] as u32,
            speed_kmh: u[14][i] as u32,
            access: u[15][i] as u16,
            class: (class & 7) as u8,
            use_: ((class >> 3) & 63) as u8,
            density: ((class >> 9) & 15) as u8,
            flags: u[17][i] as u32,
            local_index: (turn & 127) as u8,
            opp_local_index: ((turn >> 7) & 127) as u8,
            restrictions: ((turn >> 14) & 255) as u8,
            shape: index((shape >> 1) as i64, shape_count, "edge shape")?,
            reversed: shape & 1 == 1,
            stop_impact: u[27][i] as u32,
            same_name: u[28][i] as u8,
        });
    }
    tile.names = c.names;
    tile.restrictions = c.restrictions;
    Ok(tile)
}
