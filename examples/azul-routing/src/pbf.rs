//! Reading an OpenStreetMap extract (`.osm.pbf`, as Geofabrik publishes
//! them): the file is a run of blobs, each a zlib-packed protobuf block of
//! nodes, ways and relations. Only what routing needs is decoded - node
//! positions, and ways with their node ids and tags - with the crate's own
//! protobuf reader ([`crate::wire`]), so no code generator and no protobuf
//! crate is needed.

use std::io::Read;

use crate::wire::{read_varints, unzigzag, Reader, Value};

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
    let mut fields = Reader::new(bytes);
    let (mut kind, mut size) = (String::new(), 0usize);
    while let Some((number, wire)) = fields.field()? {
        match (number, wire) {
            (1, Value::Bytes(b)) => kind = String::from_utf8_lossy(b).into_owned(),
            (3, Value::Varint(v)) => size = v as usize,
            _ => {}
        }
    }
    Ok((kind, size))
}

/// A `Blob`'s content, unpacked.
fn blob_data(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut fields = Reader::new(bytes);
    while let Some((number, wire)) = fields.field()? {
        match (number, wire) {
            (1, Value::Bytes(raw)) => return Ok(raw.to_vec()),
            (3, Value::Bytes(packed)) => {
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
    let mut fields = Reader::new(bytes);
    while let Some((number, wire)) = fields.field()? {
        match (number, wire) {
            (1, Value::Bytes(table)) => {
                let mut t = Reader::new(table);
                while let Some((n, w)) = t.field()? {
                    if let (1, Value::Bytes(s)) = (n, w) {
                        strings.push(s);
                    }
                }
            }
            (2, Value::Bytes(group)) => groups.push(group),
            (17, Value::Varint(v)) => granularity = v as i64,
            (19, Value::Varint(v)) => lat_offset = v as i64,
            (20, Value::Varint(v)) => lon_offset = v as i64,
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
        let mut fields = Reader::new(group);
        while let Some((number, wire)) = fields.field()? {
            match (number, wire) {
                (1, Value::Bytes(node)) if !ways_only => block.nodes.push(plain_node(node, scale)?),
                (2, Value::Bytes(dense)) if !ways_only => dense_nodes(dense, scale, &mut block.nodes)?,
                (3, Value::Bytes(way)) => block.ways.push(read_way(way, &text)?),
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
    let mut fields = Reader::new(bytes);
    while let Some((number, wire)) = fields.field()? {
        match (number, wire) {
            (1, Value::Varint(v)) => id = unzigzag(v),
            (8, Value::Varint(v)) => lat = unzigzag(v),
            (9, Value::Varint(v)) => lon = unzigzag(v),
            _ => {}
        }
    }
    let (lat, lon) = scale.degrees(lat, lon);
    Ok((id, lat, lon))
}

/// `DenseNodes`: ids and coordinates as runs of deltas.
fn dense_nodes(bytes: &[u8], scale: Scale, out: &mut Vec<(i64, f64, f64)>) -> Result<(), String> {
    let (mut ids, mut lats, mut lons) = (Vec::new(), Vec::new(), Vec::new());
    let mut fields = Reader::new(bytes);
    while let Some((number, wire)) = fields.field()? {
        match number {
            1 => read_varints(wire, &mut ids)?,
            8 => read_varints(wire, &mut lats)?,
            9 => read_varints(wire, &mut lons)?,
            _ => {}
        }
    }
    if ids.len() != lats.len() || ids.len() != lons.len() {
        return Err("dense nodes whose ids and coordinates differ in number".into());
    }
    let (mut id, mut lat, mut lon) = (0i64, 0i64, 0i64);
    for i in 0..ids.len() {
        id += unzigzag(ids[i]);
        lat += unzigzag(lats[i]);
        lon += unzigzag(lons[i]);
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
    let mut fields = Reader::new(bytes);
    while let Some((number, wire)) = fields.field()? {
        match number {
            1 => {
                if let Value::Varint(v) = wire {
                    way.id = v as i64;
                }
            }
            2 => read_varints(wire, &mut keys)?,
            3 => read_varints(wire, &mut values)?,
            8 => {
                let mut deltas = Vec::new();
                read_varints(wire, &mut deltas)?;
                let mut id = 0i64;
                for delta in deltas {
                    id += unzigzag(delta);
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
    use crate::wire::{zigzag, Writer};

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
        let mut dense = Writer::default();
        let delta = |values: Vec<i64>| {
            let mut prev = 0;
            values
                .into_iter()
                .map(|v| {
                    let d = v - prev;
                    prev = v;
                    zigzag(d)
                })
                .collect::<Vec<u64>>()
        };
        #[allow(clippy::cast_possible_truncation)]
        let nano = |deg: f64| (deg * 1e7).round() as i64;
        dense.packed(1, delta(nodes.iter().map(|n| n.0).collect()));
        dense.packed(8, delta(nodes.iter().map(|n| nano(n.1)).collect()));
        dense.packed(9, delta(nodes.iter().map(|n| nano(n.2)).collect()));
        let mut group = Writer::default();
        group.bytes(2, &dense.buf);
        for (id, refs, tags) in ways {
            let mut way = Writer::default();
            way.uint(1, *id as u64);
            let keys: Vec<u64> = tags.iter().map(|(k, _)| index(k, &mut strings)).collect();
            let vals: Vec<u64> = tags.iter().map(|(_, v)| index(v, &mut strings)).collect();
            way.packed(2, keys);
            way.packed(3, vals);
            way.packed(8, delta(refs.clone()));
            group.bytes(3, &way.buf);
        }
        let mut table = Writer::default();
        for s in &strings {
            table.bytes(1, s.as_bytes());
        }
        let mut block = Writer::default();
        block.bytes(1, &table.buf);
        block.bytes(2, &group.buf);
        let mut zipped = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
        zipped.write_all(&block.buf).expect("zlib");
        let zipped = zipped.finish().expect("zlib");
        let mut blob = Writer::default();
        blob.uint(2, block.buf.len() as u64);
        blob.bytes(3, &zipped);
        let mut header = Writer::default();
        header.bytes(1, b"OSMData");
        header.uint(3, blob.buf.len() as u64);
        let mut file = Vec::new();
        file.extend_from_slice(&(header.buf.len() as u32).to_be_bytes());
        file.extend_from_slice(&header.buf);
        file.extend_from_slice(&blob.buf);
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
}
