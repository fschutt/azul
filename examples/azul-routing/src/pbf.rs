//! Reading an OpenStreetMap extract (`.osm.pbf`, as Geofabrik publishes
//! them): the file is a run of blobs, each a zlib-packed protobuf block of
//! nodes, ways and relations. Only what routing needs is decoded - node
//! positions, and ways with their node ids and tags - with a small protobuf
//! reader of its own, so no code generator and no protobuf crate is needed.

use std::io::Read;

/// One protobuf field as it is on the wire.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Wire<'a> {
    Varint(u64),
    Fixed64(u64),
    Bytes(&'a [u8]),
    Fixed32(u32),
}

/// Reads the fields of one protobuf message in turn.
struct Fields<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Fields<'a> {
    const fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    fn varint(&mut self) -> Result<u64, String> {
        let mut value = 0u64;
        for shift in (0..64).step_by(7) {
            let byte = *self.buf.get(self.pos).ok_or("a varint runs past the end")?;
            self.pos += 1;
            value |= u64::from(byte & 0x7f) << shift;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err("a varint is longer than ten bytes".into())
    }

    fn take(&mut self, n: usize) -> Result<&'a [u8], String> {
        let end = self.pos.checked_add(n).filter(|e| *e <= self.buf.len());
        let end = end.ok_or("a field runs past the end")?;
        let bytes = &self.buf[self.pos..end];
        self.pos = end;
        Ok(bytes)
    }

    /// The next field's number and value; `None` at the end.
    #[allow(clippy::cast_possible_truncation)] // field numbers and lengths are small
    fn next(&mut self) -> Result<Option<(u32, Wire<'a>)>, String> {
        if self.pos >= self.buf.len() {
            return Ok(None);
        }
        let key = self.varint()?;
        let number = (key >> 3) as u32;
        let wire = match key & 7 {
            0 => Wire::Varint(self.varint()?),
            1 => {
                let b = self.take(8)?;
                Wire::Fixed64(u64::from_le_bytes(b.try_into().map_err(|_| "fixed64")?))
            }
            2 => {
                let n = self.varint()? as usize;
                Wire::Bytes(self.take(n)?)
            }
            5 => {
                let b = self.take(4)?;
                Wire::Fixed32(u32::from_le_bytes(b.try_into().map_err(|_| "fixed32")?))
            }
            other => return Err(format!("wire type {other} is not read")),
        };
        Ok(Some((number, wire)))
    }
}

/// A signed varint's value (protobuf's zig-zag `sint64`).
#[allow(clippy::cast_possible_wrap)] // zig-zag folds the sign into the low bit
const fn zigzag(n: u64) -> i64 {
    (n >> 1) as i64 ^ -((n & 1) as i64)
}

/// The values of a repeated varint field: packed (one length-delimited run)
/// or one value per field.
fn varints(wire: Wire<'_>) -> Result<Vec<u64>, String> {
    match wire {
        Wire::Varint(v) => Ok(vec![v]),
        Wire::Bytes(bytes) => {
            let mut fields = Fields::new(bytes);
            let mut out = Vec::new();
            while fields.pos < bytes.len() {
                out.push(fields.varint()?);
            }
            Ok(out)
        }
        _ => Err("a repeated number is neither packed nor a varint".into()),
    }
}

/// What a block of the extract holds, in the shape routing reads it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Block {
    /// `(id, lat, lon)` of every node in the block.
    pub nodes: Vec<(i64, f64, f64)>,
    /// Every way: its id, its node ids in order, and its tags.
    pub ways: Vec<OsmWay>,
}

/// One way of the extract.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct OsmWay {
    pub id: i64,
    pub nodes: Vec<i64>,
    pub tags: Vec<(String, String)>,
}

/// Reads every data block of the extract at `reader`, handing each to
/// `on_block`. With `ways_only` the nodes are not decoded (the first of the
/// generator's two passes needs only the ways).
///
/// # Errors
/// The file is not an extract this reader can read: a broken blob, a
/// compression other than zlib, a malformed block. `on_block`'s own error
/// ends the read too.
pub fn read_blocks(
    reader: &mut dyn Read,
    ways_only: bool,
    on_block: &mut dyn FnMut(Block) -> Result<(), String>,
) -> Result<(), String> {
    loop {
        let mut size = [0u8; 4];
        match reader.read_exact(&mut size) {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(()),
            Err(e) => return Err(format!("read: {e}")),
        }
        let header = read_n(reader, u32::from_be_bytes(size) as usize)?;
        let (kind, data_size) = blob_header(&header)?;
        let blob = read_n(reader, data_size)?;
        if kind != "OSMData" {
            continue;
        }
        let block = blob_data(&blob)?;
        on_block(primitive_block(&block, ways_only)?)?;
    }
}

/// `n` bytes from `reader`; refuses a size no extract has.
fn read_n(reader: &mut dyn Read, n: usize) -> Result<Vec<u8>, String> {
    // The format caps a blob at 32 MiB.
    if n > 64 * 1024 * 1024 {
        return Err(format!("a blob of {n} bytes"));
    }
    let mut buf = vec![0u8; n];
    reader.read_exact(&mut buf).map_err(|e| format!("read: {e}"))?;
    Ok(buf)
}

/// A `BlobHeader`: the blob's type and size.
#[allow(clippy::cast_possible_truncation)] // a blob's size fits
fn blob_header(bytes: &[u8]) -> Result<(String, usize), String> {
    let mut fields = Fields::new(bytes);
    let (mut kind, mut size) = (String::new(), 0usize);
    while let Some((number, wire)) = fields.next()? {
        match (number, wire) {
            (1, Wire::Bytes(b)) => kind = String::from_utf8_lossy(b).into_owned(),
            (3, Wire::Varint(v)) => size = v as usize,
            _ => {}
        }
    }
    Ok((kind, size))
}

/// A `Blob`'s content, unpacked.
fn blob_data(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut fields = Fields::new(bytes);
    while let Some((number, wire)) = fields.next()? {
        match (number, wire) {
            (1, Wire::Bytes(raw)) => return Ok(raw.to_vec()),
            (3, Wire::Bytes(packed)) => {
                let mut out = Vec::new();
                flate2::read::ZlibDecoder::new(packed)
                    .read_to_end(&mut out)
                    .map_err(|e| format!("zlib: {e}"))?;
                return Ok(out);
            }
            (4..=7, _) => return Err("a blob packed with other than zlib".into()),
            _ => {}
        }
    }
    Err("an empty blob".into())
}

/// A `PrimitiveBlock`: its string table, coordinate scale and groups.
#[allow(clippy::cast_possible_wrap, clippy::cast_possible_truncation)] // protobuf's int32 / int64
fn primitive_block(bytes: &[u8], ways_only: bool) -> Result<Block, String> {
    let mut strings: Vec<&[u8]> = Vec::new();
    let mut groups: Vec<&[u8]> = Vec::new();
    let (mut granularity, mut lat_offset, mut lon_offset) = (100i64, 0i64, 0i64);
    let mut fields = Fields::new(bytes);
    while let Some((number, wire)) = fields.next()? {
        match (number, wire) {
            (1, Wire::Bytes(table)) => {
                let mut t = Fields::new(table);
                while let Some((n, w)) = t.next()? {
                    if let (1, Wire::Bytes(s)) = (n, w) {
                        strings.push(s);
                    }
                }
            }
            (2, Wire::Bytes(group)) => groups.push(group),
            (17, Wire::Varint(v)) => granularity = v as i64,
            (19, Wire::Varint(v)) => lat_offset = v as i64,
            (20, Wire::Varint(v)) => lon_offset = v as i64,
            _ => {}
        }
    }
    let scale = Scale {
        granularity,
        lat_offset,
        lon_offset,
    };
    let text = |index: u64| {
        strings
            .get(usize::try_from(index).unwrap_or(usize::MAX))
            .map(|s| String::from_utf8_lossy(s).into_owned())
            .unwrap_or_default()
    };
    let mut block = Block::default();
    for group in groups {
        let mut fields = Fields::new(group);
        while let Some((number, wire)) = fields.next()? {
            match (number, wire) {
                (1, Wire::Bytes(node)) if !ways_only => block.nodes.push(plain_node(node, scale)?),
                (2, Wire::Bytes(dense)) if !ways_only => dense_nodes(dense, scale, &mut block.nodes)?,
                (3, Wire::Bytes(way)) => block.ways.push(read_way(way, &text)?),
                _ => {}
            }
        }
    }
    Ok(block)
}

/// How a block stores coordinates: nanodegrees are
/// `offset + granularity * value`.
#[derive(Debug, Clone, Copy)]
struct Scale {
    granularity: i64,
    lat_offset: i64,
    lon_offset: i64,
}

impl Scale {
    #[allow(clippy::cast_precision_loss)] // nanodegrees as degrees
    fn degrees(self, lat: i64, lon: i64) -> (f64, f64) {
        (
            (self.lat_offset + self.granularity * lat) as f64 * 1e-9,
            (self.lon_offset + self.granularity * lon) as f64 * 1e-9,
        )
    }
}

/// A `Node` stored on its own.
fn plain_node(bytes: &[u8], scale: Scale) -> Result<(i64, f64, f64), String> {
    let (mut id, mut lat, mut lon) = (0i64, 0i64, 0i64);
    let mut fields = Fields::new(bytes);
    while let Some((number, wire)) = fields.next()? {
        match (number, wire) {
            (1, Wire::Varint(v)) => id = zigzag(v),
            (8, Wire::Varint(v)) => lat = zigzag(v),
            (9, Wire::Varint(v)) => lon = zigzag(v),
            _ => {}
        }
    }
    let (lat, lon) = scale.degrees(lat, lon);
    Ok((id, lat, lon))
}

/// `DenseNodes`: ids and coordinates as runs of deltas.
fn dense_nodes(bytes: &[u8], scale: Scale, out: &mut Vec<(i64, f64, f64)>) -> Result<(), String> {
    let (mut ids, mut lats, mut lons) = (Vec::new(), Vec::new(), Vec::new());
    let mut fields = Fields::new(bytes);
    while let Some((number, wire)) = fields.next()? {
        match number {
            1 => ids.extend(varints(wire)?),
            8 => lats.extend(varints(wire)?),
            9 => lons.extend(varints(wire)?),
            _ => {}
        }
    }
    if ids.len() != lats.len() || ids.len() != lons.len() {
        return Err("dense nodes whose ids and coordinates differ in number".into());
    }
    let (mut id, mut lat, mut lon) = (0i64, 0i64, 0i64);
    for i in 0..ids.len() {
        id += zigzag(ids[i]);
        lat += zigzag(lats[i]);
        lon += zigzag(lons[i]);
        let (la, lo) = scale.degrees(lat, lon);
        out.push((id, la, lo));
    }
    Ok(())
}

/// A `Way`: its id, tags and node ids (deltas).
#[allow(clippy::cast_possible_wrap)] // protobuf's int64
fn read_way(bytes: &[u8], text: &dyn Fn(u64) -> String) -> Result<OsmWay, String> {
    let mut way = OsmWay::default();
    let (mut keys, mut values) = (Vec::new(), Vec::new());
    let mut fields = Fields::new(bytes);
    while let Some((number, wire)) = fields.next()? {
        match number {
            1 => {
                if let Wire::Varint(v) = wire {
                    way.id = v as i64;
                }
            }
            2 => keys.extend(varints(wire)?),
            3 => values.extend(varints(wire)?),
            8 => {
                let mut id = 0i64;
                for delta in varints(wire)? {
                    id += zigzag(delta);
                    way.nodes.push(id);
                }
            }
            _ => {}
        }
    }
    way.tags = keys
        .iter()
        .zip(values.iter())
        .map(|(k, v)| (text(*k), text(*v)))
        .collect();
    Ok(way)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;

    /// A tiny protobuf writer, enough to build an extract in a test.
    #[derive(Default)]
    pub(crate) struct Out(pub(crate) Vec<u8>);

    impl Out {
        pub(crate) fn varint(&mut self, mut v: u64) {
            loop {
                let byte = (v & 0x7f) as u8;
                v >>= 7;
                if v == 0 {
                    self.0.push(byte);
                    return;
                }
                self.0.push(byte | 0x80);
            }
        }
        pub(crate) fn key(&mut self, number: u32, wire: u64) {
            self.varint((u64::from(number) << 3) | wire);
        }
        pub(crate) fn uint(&mut self, number: u32, v: u64) {
            self.key(number, 0);
            self.varint(v);
        }
        pub(crate) fn bytes(&mut self, number: u32, b: &[u8]) {
            self.key(number, 2);
            self.varint(b.len() as u64);
            self.0.extend_from_slice(b);
        }
        pub(crate) fn packed(&mut self, number: u32, values: &[u64]) {
            let mut inner = Out::default();
            for v in values {
                inner.varint(*v);
            }
            self.bytes(number, &inner.0);
        }
    }

    pub(crate) const fn zig(v: i64) -> u64 {
        ((v << 1) ^ (v >> 63)) as u64
    }

    /// An extract with `nodes` (dense, granularity 100) and `ways` (tags as
    /// key / value pairs), in one zlib blob.
    pub(crate) fn extract(nodes: &[(i64, f64, f64)], ways: &[(i64, Vec<i64>, Vec<(&str, &str)>)]) -> Vec<u8> {
        use std::io::Write;
        let mut strings: Vec<String> = vec![String::new()];
        let mut index = |s: &str, strings: &mut Vec<String>| {
            if let Some(i) = strings.iter().position(|x| x == s) {
                return i as u64;
            }
            strings.push(s.to_string());
            (strings.len() - 1) as u64
        };
        let mut dense = Out::default();
        let delta = |values: Vec<i64>| {
            let mut prev = 0;
            values
                .into_iter()
                .map(|v| {
                    let d = v - prev;
                    prev = v;
                    zig(d)
                })
                .collect::<Vec<u64>>()
        };
        #[allow(clippy::cast_possible_truncation)]
        let nano = |deg: f64| (deg * 1e7).round() as i64;
        dense.packed(1, &delta(nodes.iter().map(|n| n.0).collect()));
        dense.packed(8, &delta(nodes.iter().map(|n| nano(n.1)).collect()));
        dense.packed(9, &delta(nodes.iter().map(|n| nano(n.2)).collect()));
        let mut group = Out::default();
        group.bytes(2, &dense.0);
        for (id, refs, tags) in ways {
            let mut way = Out::default();
            way.uint(1, *id as u64);
            let keys: Vec<u64> = tags.iter().map(|(k, _)| index(k, &mut strings)).collect();
            let vals: Vec<u64> = tags.iter().map(|(_, v)| index(v, &mut strings)).collect();
            way.packed(2, &keys);
            way.packed(3, &vals);
            way.packed(8, &delta(refs.clone()));
            group.bytes(3, &way.0);
        }
        let mut table = Out::default();
        for s in &strings {
            table.bytes(1, s.as_bytes());
        }
        let mut block = Out::default();
        block.bytes(1, &table.0);
        block.bytes(2, &group.0);
        let mut zipped = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        zipped.write_all(&block.0).expect("zlib");
        let zipped = zipped.finish().expect("zlib");
        let mut blob = Out::default();
        blob.uint(2, block.0.len() as u64);
        blob.bytes(3, &zipped);
        let mut header = Out::default();
        header.bytes(1, b"OSMData");
        header.uint(3, blob.0.len() as u64);
        let mut file = Vec::new();
        file.extend_from_slice(&(header.0.len() as u32).to_be_bytes());
        file.extend_from_slice(&header.0);
        file.extend_from_slice(&blob.0);
        file
    }

    #[test]
    fn an_extracts_dense_nodes_and_ways_read_back_as_written() {
        let file = extract(
            &[(10, 43.7384, 7.4246), (11, 43.7390, 7.4250), (15, 43.7401, 7.4262)],
            &[(500, vec![10, 11, 15], vec![("highway", "residential"), ("name", "Rue A")])],
        );
        let mut blocks = Vec::new();
        read_blocks(&mut &file[..], false, &mut |b| {
            blocks.push(b);
            Ok(())
        })
        .expect("the extract reads");
        assert_eq!(blocks.len(), 1);
        let block = &blocks[0];
        assert_eq!(block.nodes.len(), 3);
        assert_eq!(block.nodes[2].0, 15);
        assert!((block.nodes[2].1 - 43.7401).abs() < 1e-7, "{:?}", block.nodes[2]);
        assert!((block.nodes[2].2 - 7.4262).abs() < 1e-7, "{:?}", block.nodes[2]);
        assert_eq!(block.ways[0].id, 500);
        assert_eq!(block.ways[0].nodes, vec![10, 11, 15]);
        assert!(block.ways[0].tags.contains(&("highway".into(), "residential".into())));
    }

    #[test]
    fn the_first_pass_reads_the_ways_and_skips_the_nodes() {
        let file = extract(&[(1, 1.0, 1.0), (2, 1.0, 1.001)], &[(9, vec![1, 2], vec![("highway", "path")])]);
        let mut blocks = Vec::new();
        read_blocks(&mut &file[..], true, &mut |b| {
            blocks.push(b);
            Ok(())
        })
        .expect("the extract reads");
        assert!(blocks[0].nodes.is_empty());
        assert_eq!(blocks[0].ways.len(), 1);
    }

    #[test]
    fn a_signed_varint_folds_its_sign_into_the_low_bit() {
        for v in [0, 1, -1, 2, -2, 1 << 40, -(1 << 40), i64::MAX, i64::MIN] {
            assert_eq!(zigzag(zig(v)), v);
        }
    }
}
