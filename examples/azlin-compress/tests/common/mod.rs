//! Helpers shared by the integration tests: a seeded generator (the property tests are
//! repeatable without a dependency) and inputs of each shape.

#![allow(dead_code)]

/// xorshift64*: small, seeded, good enough to make arbitrary bytes.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.wrapping_mul(0x9E37_79B9_7F4A_7C15) | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_F491_4F6C_DD1D)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }

    pub fn bytes(&mut self, len: usize) -> Vec<u8> {
        (0..len).map(|_| self.next() as u8).collect()
    }
}

const WORDS: &[&str] = &[
    "the", "drive", "keeps", "every", "file", "small", "and", "safe", "on", "this", "computer",
    "before", "it", "leaves", "for", "the", "storage", "nodes", "which", "never", "see", "a",
    "plaintext", "byte", "of", "what", "you", "store", "there", "photos", "letters", "invoices",
];

/// Prose-like text from a small vocabulary.
pub fn text(rng: &mut Rng, len: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(len + 16);
    while out.len() < len {
        out.extend_from_slice(WORDS[rng.below(WORDS.len() as u64) as usize].as_bytes());
        out.push(if rng.below(12) == 0 { b'\n' } else { b' ' });
    }
    out.truncate(len);
    out
}

/// A typical small JSON file: a settings file with a schema reference.
pub fn settings_json() -> Vec<u8> {
    br#"{
  "$schema": "http://json-schema.org/draft-07/schema#",
  "title": "Settings",
  "type": "object",
  "properties": {
    "name": { "type": "string", "description": "The name of the project" },
    "version": { "type": "string" },
    "private": { "type": "boolean", "default": false },
    "dependencies": { "type": "object", "additionalProperties": { "type": "string" } }
  },
  "required": ["name", "version"]
}
"#
    .to_vec()
}

/// JSON records.
pub fn json(rng: &mut Rng, records: usize) -> Vec<u8> {
    let mut out = String::from("[");
    for i in 0..records {
        if i > 0 {
            out.push(',');
        }
        out.push_str(&format!(
            "{{\"id\":{},\"name\":\"user{}\",\"active\":{},\"score\":{}}}",
            i,
            rng.below(10_000),
            rng.below(2) == 1,
            rng.below(1000)
        ));
    }
    out.push(']');
    out.into_bytes()
}

/// Inputs of every shape the codec stage meets: empty, tiny, random, repetitive, text, and
/// a mix whose compressible part starts after an incompressible one.
pub fn shapes(rng: &mut Rng, segment: usize) -> Vec<Vec<u8>> {
    let mut mixed = rng.bytes(segment * 2);
    mixed.extend_from_slice(&text(rng, segment * 12));
    vec![
        Vec::new(),
        vec![0x42],
        rng.bytes(segment - 1),
        rng.bytes(segment),
        rng.bytes(segment + 1),
        vec![0u8; segment * 3 + 7],
        text(rng, segment * 2 + 333),
        json(rng, 200),
        mixed,
    ]
}

/// A minimal PDF around `streams` (each one an object with its own dictionary), with a text
/// page of `pages` lines: enough structure for detection and the JPEG scan.
pub fn pdf(streams: &[(&str, &[u8])], lines: usize) -> Vec<u8> {
    let mut out = b"%PDF-1.7\n%\xE2\xE3\xCF\xD3\n".to_vec();
    let mut content = String::new();
    for i in 0..lines {
        content.push_str(&format!("BT /F1 12 Tf 72 {} Td (Line {} of the report) Tj ET\n", 720 - i % 60 * 12, i));
    }
    let mut objects: Vec<(String, Vec<u8>)> = vec![(String::from("/Length ") + &content.len().to_string(), content.into_bytes())];
    for (dict, data) in streams {
        objects.push((format!("{dict} /Length {}", data.len()), data.to_vec()));
    }
    for (n, (dict, data)) in objects.iter().enumerate() {
        out.extend_from_slice(format!("{} 0 obj\n<< {} >>\nstream\n", n + 1, dict).as_bytes());
        out.extend_from_slice(data);
        out.extend_from_slice(b"\nendstream\nendobj\n");
    }
    out.extend_from_slice(b"trailer\n<< /Root 1 0 R >>\n%%EOF\n");
    out
}

/// A ZIP archive whose members are all stored (method 0).
pub fn stored_zip(members: &[(&str, &[u8])]) -> Vec<u8> {
    zip(members, None)
}

/// A ZIP archive whose members are deflated (method 8) at `level`, as Word, Excel and
/// PowerPoint write them.
pub fn deflated_zip(members: &[(&str, &[u8])], level: u32) -> Vec<u8> {
    zip(members, Some(level))
}

/// A ZIP archive with real CRC-32s; members stored, or deflated at `level`.
fn zip(members: &[(&str, &[u8])], level: Option<u32>) -> Vec<u8> {
    let method: u8 = if level.is_some() { 8 } else { 0 };
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in members {
        let offset = out.len() as u32;
        let crc = crc32fast::hash(data);
        let packed = match level {
            Some(level) => deflate(data, level),
            None => data.to_vec(),
        };
        out.extend_from_slice(b"PK\x03\x04");
        out.extend_from_slice(&[20, 0, 0, 0, method, 0, 0, 0, 0, 0]);
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(&packed);
        central.extend_from_slice(b"PK\x01\x02");
        central.extend_from_slice(&[20, 0, 20, 0, 0, 0, method, 0, 0, 0, 0, 0]);
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        central.extend_from_slice(&(data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 12]);
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let central_offset = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(b"PK\x05\x06\x00\x00\x00\x00");
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(members.len() as u16).to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&central_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

/// Text whose repeats lie farther apart than deflate's 32 KiB window: a deflate encoder cannot
/// reach them and zstd can, so rebuilding a deflate stream of it from its plain text pays.
pub fn far_repeats(rng: &mut Rng, block: usize, times: usize) -> Vec<u8> {
    text(rng, block).repeat(times)
}

/// `data` as raw deflate at `level` (0 to 9).
pub fn deflate(data: &[u8], level: u32) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::DeflateEncoder::new(Vec::new(), flate2::Compression::new(level));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// `data` as a zlib stream at `level`.
pub fn zlib(data: &[u8], level: u32) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::new(level));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

/// `data` as a one-member gzip file at `level`.
pub fn gzip(data: &[u8], level: u32) -> Vec<u8> {
    use std::io::Write;
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::new(level));
    encoder.write_all(data).unwrap();
    encoder.finish().unwrap()
}

fn png_chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    out.extend_from_slice(kind);
    out.extend_from_slice(data);
    let mut crc = crc32fast::Hasher::new();
    crc.update(kind);
    crc.update(data);
    out.extend_from_slice(&crc.finalize().to_be_bytes());
}

/// An RGB PNG, `width` x `height`, whose rows repeat a random tile of `tile_rows` rows (a tile
/// over 32 KiB puts its repeats beyond deflate's window), its zlib stream cut into IDAT chunks
/// of `chunk` bytes.
pub fn png(rng: &mut Rng, width: u32, height: u32, tile_rows: u32, chunk: usize) -> Vec<u8> {
    let row_len = 1 + width as usize * 3;
    let tile: Vec<Vec<u8>> = (0..tile_rows)
        .map(|_| {
            let mut row = rng.bytes(row_len);
            row[0] = 0; // filter type None
            row
        })
        .collect();
    let mut raw = Vec::with_capacity(row_len * height as usize);
    for y in 0..height {
        raw.extend_from_slice(&tile[(y % tile_rows) as usize]);
    }
    let stream = zlib(&raw, 6);
    let mut out = b"\x89PNG\r\n\x1A\n".to_vec();
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 2, 0, 0, 0]);
    png_chunk(&mut out, b"IHDR", &header);
    png_chunk(&mut out, b"tEXt", b"Comment\x00made for the azlin-compress tests");
    for part in stream.chunks(chunk) {
        png_chunk(&mut out, b"IDAT", part);
    }
    png_chunk(&mut out, b"IEND", b"");
    out
}
