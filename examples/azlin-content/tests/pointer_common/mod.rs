//! Helpers the pointer test files share: a seeded generator (SplitMix64, no `rand`), hex, the fixtures
//! (the pointers of the golden vectors and the keys that sign them), an INDEPENDENT reference encoder
//! written from the documented layout of the wire form (a mini CBOR emitter of this file, not the
//! library's writer), the doubles of the two seams (`ObjectSealer`, `DeviceDirectory`), a random
//! generator of valid pointers, and the tools that edit a pointer at the level of its CBOR value (so the
//! tests can build the bytes of pointers the library refuses to write).
//!
//! The vector files are written only under `UPDATE_VECTORS=1`, never when `CI` is set (rule 10 of the
//! lanes' common rules); otherwise the test compares the file with the table and fails with the first
//! difference.
#![allow(dead_code)]

pub mod fuzz_entry;

use std::cell::{Cell, RefCell};
use std::path::PathBuf;

use azlin_cbor::{decode, encode, Limits, Map, Value};
use azlin_content::pointer::{
    ChunkList, ChunkListRef, DerivedRef, ObjectSealer, PointerV2, Preview, SealError, StreamRef,
    TransformRef, UnknownFields,
};
use azlin_content::{ChunkLen, ChunkRef, FileSize, FileTime, Full, PackId, PackOffset};
use azlin_keys::{DeviceId, MapDirectory, Secret, SignKeypair, Signature, SpaceId, UnixTime};
use zeroize::Zeroizing;

// ---------------------------------------------------------------------------------------------
// The seeded generator and hex
// ---------------------------------------------------------------------------------------------

/// SplitMix64: the seeded generator of every test stream. A failing case prints its seed and
/// `SplitMix64::new(seed)` replays it.
pub struct SplitMix64(u64);

impl SplitMix64 {
    pub fn new(seed: u64) -> SplitMix64 {
        SplitMix64(seed)
    }

    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    /// A number below `n` (`n > 0`); the modulo bias is irrelevant for tests.
    pub fn below(&mut self, n: u64) -> u64 {
        self.next_u64() % n
    }

    pub fn fill(&mut self, out: &mut [u8]) {
        for piece in out.chunks_mut(8) {
            let bytes = self.next_u64().to_le_bytes();
            let n = piece.len();
            piece.copy_from_slice(&bytes[..n]);
        }
    }

    pub fn bytes(&mut self, n: usize) -> Vec<u8> {
        let mut v = vec![0u8; n];
        self.fill(&mut v);
        v
    }

    pub fn array<const N: usize>(&mut self) -> [u8; N] {
        let mut a = [0u8; N];
        self.fill(&mut a);
        a
    }

    pub fn chance(&mut self, percent: u64) -> bool {
        self.below(100) < percent
    }
}

pub fn to_hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

pub fn from_hex(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2), "odd hex length");
    (0..text.len() / 2)
        .map(|i| u8::from_str_radix(&text[2 * i..2 * i + 2], 16).expect("a hex digit"))
        .collect()
}

pub const OBJECT: Limits = Limits::OBJECT;

// ---------------------------------------------------------------------------------------------
// The vector files: one object per line, written only under UPDATE_VECTORS=1
// ---------------------------------------------------------------------------------------------

pub fn vector_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(format!("tests/vectors/{name}.json"))
}

/// The text of a vector file: `[`, one line per row, `]`.
pub fn render_rows(rows: &[String]) -> String {
    let mut out = String::from("[\n");
    for (i, row) in rows.iter().enumerate() {
        out.push_str("  ");
        out.push_str(row);
        out.push_str(if i + 1 < rows.len() { ",\n" } else { "\n" });
    }
    out.push_str("]\n");
    out
}

/// Writes the vector file under `UPDATE_VECTORS=1` (refused when `CI` is set), else compares the file
/// with `want` and fails with the first difference.
pub fn check_vector_file(name: &str, want: &str) {
    let path = vector_path(name);
    if std::env::var("UPDATE_VECTORS").is_ok_and(|v| v == "1") {
        assert!(
            std::env::var_os("CI").is_none(),
            "UPDATE_VECTORS=1 is refused when CI is set"
        );
        std::fs::create_dir_all(path.parent().expect("a parent folder")).expect("tests/vectors");
        std::fs::write(&path, want).expect("writes the vector file");
        return;
    }
    let have = std::fs::read_to_string(&path).unwrap_or_else(|_| {
        panic!("tests/vectors/{name}.json is missing: write it once with UPDATE_VECTORS=1 (never in CI)")
    });
    if have != want {
        let at = have
            .bytes()
            .zip(want.bytes())
            .position(|(a, b)| a != b)
            .unwrap_or_else(|| have.len().min(want.len()));
        let line = have[..at.min(have.len())].matches('\n').count() + 1;
        panic!(
            "tests/vectors/{name}.json differs from the table of the test, first at byte {at} (line {line}); UPDATE_VECTORS=1 rewrites it, never in CI"
        );
    }
}

/// The rows of a vector file as the lines between `[` and `]`, without their trailing commas.
pub fn read_rows(name: &str) -> Vec<String> {
    let text = std::fs::read_to_string(vector_path(name)).expect("the vector file exists");
    text.lines()
        .map(str::trim)
        .filter(|l| l.starts_with('{'))
        .map(|l| l.trim_end_matches(',').to_string())
        .collect()
}

/// The string field `"key": "value"` of a row (values are hex or plain words: no escapes).
pub fn field(row: &str, key: &str) -> String {
    let marker = format!("\"{key}\": \"");
    let start = row
        .find(&marker)
        .unwrap_or_else(|| panic!("no field {key} in {row}"))
        + marker.len();
    let end = row[start..].find('"').expect("a closing quote") + start;
    row[start..end].to_string()
}

/// The boolean field `"key": true` of a row.
pub fn flag(row: &str, key: &str) -> bool {
    row.contains(&format!("\"{key}\": true"))
}

// ---------------------------------------------------------------------------------------------
// A mini CBOR emitter: the reference of the wire form, written from the documented layout
// ---------------------------------------------------------------------------------------------

pub mod cbor {
    /// The initial byte and argument of a data item in the shortest form.
    pub fn head(major: u8, n: u64) -> Vec<u8> {
        let m = major << 5;
        if n < 24 {
            vec![m | n as u8]
        } else if n < 0x100 {
            vec![m | 24, n as u8]
        } else if n < 0x1_0000 {
            let mut v = vec![m | 25];
            v.extend_from_slice(&(n as u16).to_be_bytes());
            v
        } else if n < 0x1_0000_0000 {
            let mut v = vec![m | 26];
            v.extend_from_slice(&(n as u32).to_be_bytes());
            v
        } else {
            let mut v = vec![m | 27];
            v.extend_from_slice(&n.to_be_bytes());
            v
        }
    }

    pub fn uint(n: u64) -> Vec<u8> {
        head(0, n)
    }

    /// A signed integer: an unsigned one, or `-1 - n` as major type 1.
    pub fn int(i: i64) -> Vec<u8> {
        if i >= 0 {
            head(0, i as u64)
        } else {
            head(1, (-1 - i) as u64)
        }
    }

    pub fn bytes(b: &[u8]) -> Vec<u8> {
        let mut v = head(2, b.len() as u64);
        v.extend_from_slice(b);
        v
    }

    pub fn text(s: &str) -> Vec<u8> {
        let mut v = head(3, s.len() as u64);
        v.extend_from_slice(s.as_bytes());
        v
    }

    pub fn array(items: &[Vec<u8>]) -> Vec<u8> {
        let mut v = head(4, items.len() as u64);
        for item in items {
            v.extend_from_slice(item);
        }
        v
    }

    /// A map with unsigned keys, the keys in the order of their encodings.
    pub fn map(entries: &[(u64, Vec<u8>)]) -> Vec<u8> {
        let mut keyed: Vec<(Vec<u8>, &Vec<u8>)> =
            entries.iter().map(|(k, v)| (uint(*k), v)).collect();
        keyed.sort_by(|a, b| a.0.cmp(&b.0));
        let mut v = head(5, keyed.len() as u64);
        for (key, value) in keyed {
            v.extend_from_slice(&key);
            v.extend_from_slice(value);
        }
        v
    }
}

/// One chunk reference as the reference emitter writes it.
pub fn ref_bytes(r: &ChunkRef) -> Vec<u8> {
    cbor::array(&[
        cbor::bytes(r.full.expose()),
        cbor::uint(u64::from(r.raw_len.get())),
        cbor::bytes(r.pack.as_bytes()),
        cbor::uint(u64::from(r.offset.get())),
        cbor::uint(u64::from(r.stored_len.get())),
    ])
}

fn refs_bytes(refs: &[ChunkRef]) -> Vec<u8> {
    cbor::array(&refs.iter().map(ref_bytes).collect::<Vec<_>>())
}

/// The bytes of `p` by the documented layout, without the library's writer. `with_sig` false is the
/// body the signature covers.
pub fn reference_encode(p: &PointerV2, with_sig: bool) -> Vec<u8> {
    let chunks = match &p.chunks {
        ChunkList::Inline(refs) => refs_bytes(refs),
        ChunkList::Object(r) => cbor::map(&[
            (1, cbor::bytes(&r.name)),
            (2, cbor::uint(u64::from(r.count))),
            (3, cbor::uint(r.total_raw.get())),
        ]),
    };
    let mut e: Vec<(u64, Vec<u8>)> = vec![
        (1, cbor::uint(2)),
        (2, cbor::uint(p.size.get())),
        (
            3,
            cbor::array(&[
                cbor::int(p.mtime.secs()),
                cbor::uint(u64::from(p.mtime.nanos())),
            ]),
        ),
        (4, cbor::uint(u64::from(p.mode))),
        (5, cbor::bytes(&p.blake3)),
        (7, chunks),
        (8, cbor::bytes(p.author_device.as_bytes())),
        (15, cbor::uint(p.created_at.as_secs())),
    ];
    if let Some(t) = &p.transform {
        e.push((
            6,
            cbor::array(&[
                cbor::uint(u64::from(t.id)),
                cbor::uint(t.out_len.get()),
                encode(&t.params, &OBJECT).expect("the params encode"),
            ]),
        ));
    }
    if !p.previews.is_empty() {
        let items: Vec<Vec<u8>> = p
            .previews
            .iter()
            .map(|v| {
                cbor::array(&[
                    cbor::uint(u64::from(v.kind)),
                    cbor::uint(u64::from(v.w)),
                    cbor::uint(u64::from(v.h)),
                    refs_bytes(&v.chunks),
                ])
            })
            .collect();
        e.push((9, cbor::array(&items)));
    }
    if !p.streams.is_empty() {
        let items: Vec<Vec<u8>> = p
            .streams
            .iter()
            .map(|s| {
                cbor::array(&[
                    cbor::uint(u64::from(s.kind)),
                    cbor::uint(u64::from(s.w)),
                    cbor::uint(u64::from(s.h)),
                    cbor::uint(u64::from(s.bandwidth)),
                    refs_bytes(&s.chunks),
                ])
            })
            .collect();
        e.push((10, cbor::array(&items)));
    }
    if !p.xattrs.is_empty() {
        let items: Vec<Vec<u8>> = p
            .xattrs
            .iter()
            .map(|(n, v)| cbor::array(&[cbor::text(n), cbor::bytes(v)]))
            .collect();
        e.push((11, cbor::array(&items)));
    }
    if !p.labels.is_empty() {
        let items: Vec<Vec<u8>> = p.labels.iter().map(|l| cbor::text(l)).collect();
        e.push((12, cbor::array(&items)));
    }
    if !p.derived.is_empty() {
        let items: Vec<Vec<u8>> = p
            .derived
            .iter()
            .map(|d| {
                cbor::array(&[
                    cbor::text(&d.extractor),
                    cbor::text(&d.model_id),
                    cbor::text(&d.model_version),
                    ref_bytes(&d.record),
                ])
            })
            .collect();
        e.push((13, cbor::array(&items)));
    }
    for (key, value) in p.unknown.iter() {
        e.push((
            key,
            encode(value, &OBJECT).expect("an unknown value encodes"),
        ));
    }
    if !p.crit.is_empty() {
        let items: Vec<Vec<u8>> = p.crit.iter().map(|k| cbor::uint(*k)).collect();
        e.push((0, cbor::array(&items)));
    }
    if with_sig {
        if let Some(sig) = &p.sig {
            e.push((14, cbor::bytes(sig.as_bytes())));
        }
    }
    cbor::map(&e)
}

/// What WRITING `p` and what READING its reference encoding say (the errors, `None` for a pointer that is
/// accepted): the writer never writes what the reader would refuse, and the reader refuses what the
/// writer would not write, so the two agree.
pub fn both(
    p: &PointerV2,
) -> (
    Option<azlin_content::pointer::PointerError>,
    Option<azlin_content::pointer::PointerError>,
) {
    let written = p.to_cbor().err();
    let read = PointerV2::from_cbor(&reference_encode(p, true), &OBJECT).err();
    (written, read)
}

/// The preimage the signature covers, spelled out: the domain, a NUL, the space id, the body.
pub fn reference_preimage(space: &SpaceId, p: &PointerV2) -> Vec<u8> {
    let mut m = b"azlin-pointer-v2".to_vec();
    m.push(0);
    m.extend_from_slice(space.as_bytes());
    m.extend_from_slice(&reference_encode(p, false));
    m
}

// ---------------------------------------------------------------------------------------------
// Keys, ids, the directory
// ---------------------------------------------------------------------------------------------

pub fn space() -> SpaceId {
    SpaceId::from_bytes([0x53; 16])
}

pub fn other_space() -> SpaceId {
    SpaceId::from_bytes([0x54; 16])
}

pub fn author_id() -> DeviceId {
    DeviceId::from_bytes([0xA1; 16])
}

pub fn other_id() -> DeviceId {
    DeviceId::from_bytes([0xA2; 16])
}

pub fn author_key() -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes([0x41; 32]))
}

pub fn other_key() -> SignKeypair {
    SignKeypair::from_seed(&Secret::from_bytes([0x42; 32]))
}

/// A directory that knows the author (active at every time and position) and, as a second device, the
/// other key.
pub fn directory() -> MapDirectory {
    let mut d = MapDirectory::new();
    d.insert_active(author_id(), author_key().verify_key());
    d.insert_active(other_id(), other_key().verify_key());
    d
}

// ---------------------------------------------------------------------------------------------
// The fixtures
// ---------------------------------------------------------------------------------------------

/// A chunk reference of a deterministic look: `full` and the pack id repeat `tag`, the offset is
/// `tag * 4096`.
pub fn chunk(tag: u8, raw: u32, stored: u32) -> ChunkRef {
    ChunkRef {
        full: Full::from_bytes([tag; 32]),
        raw_len: ChunkLen::new(raw).expect("a raw length"),
        pack: PackId::from_bytes([tag; 16]),
        offset: PackOffset::new(u32::from(tag) * 4096),
        stored_len: ChunkLen::new(stored).expect("a stored length"),
    }
}

/// The `i`-th chunk of the huge fixture: distinct hashes, raw length `1000 + i % 7`.
pub fn chunk_i(i: u32) -> ChunkRef {
    let mut full = [0x77u8; 32];
    full[..4].copy_from_slice(&i.to_le_bytes());
    let raw = 1000 + i % 7;
    ChunkRef {
        full: Full::from_bytes(full),
        raw_len: ChunkLen::new(raw).expect("a raw length"),
        pack: PackId::from_bytes([(i / 64) as u8; 16]),
        offset: PackOffset::new((i % 64) * 2048),
        stored_len: ChunkLen::new(raw + 41).expect("a stored length"),
    }
}

pub fn huge_chunks(n: u32) -> Vec<ChunkRef> {
    let mut v = Vec::with_capacity(n as usize);
    for i in 0..n {
        v.push(chunk_i(i));
    }
    v
}

pub fn total_raw(refs: &[ChunkRef]) -> u64 {
    refs.iter().map(|r| u64::from(r.raw_len.get())).sum()
}

fn base(size: u64, chunks: ChunkList) -> PointerV2 {
    PointerV2 {
        size: FileSize::new(size),
        mtime: FileTime::new(1_700_000_000, 0).expect("a time"),
        mode: 0o644,
        blake3: *blake3::hash(b"the original file").as_bytes(),
        transform: None,
        chunks,
        author_device: author_id(),
        sig: None,
        previews: Vec::new(),
        streams: Vec::new(),
        xattrs: Vec::new(),
        labels: Vec::new(),
        derived: Vec::new(),
        created_at: UnixTime::from_secs(1_700_000_100),
        unknown: UnknownFields::none(),
        crit: Vec::new(),
    }
}

/// The smallest pointer: an empty file (size 0, no chunks). Its bytes are derived by hand in
/// `pointer_vectors.rs`, so the `blake3` is a plain pattern.
pub fn minimal_empty() -> PointerV2 {
    let mut p = base(0, ChunkList::Inline(Vec::new()));
    p.blake3 = [0x10; 32];
    p
}

/// A small file in one chunk.
pub fn minimal_one_chunk() -> PointerV2 {
    let mut p = base(1000, ChunkList::Inline(vec![chunk(0x21, 1000, 1041)]));
    p.mtime = FileTime::new(1_700_000_000, 123_456_789).expect("a time");
    p.mode = 0o600;
    p.blake3 = [0x11; 32];
    p
}

/// Every field populated, none unknown: a transform with parameters, three chunks, two previews, two
/// streams, two attributes, two labels, two derived records, a time before 1970.
pub fn full() -> PointerV2 {
    let params = Value::Map(
        Map::from_entries(vec![
            (Value::Uint(1), Value::Uint(9)),
            (Value::Uint(2), Value::Text("preflate".to_string())),
        ])
        .expect("a map"),
    );
    let mut p = base(
        3_000_000,
        ChunkList::Inline(vec![
            chunk(0x41, 1_000_000, 1_000_041),
            chunk(0x42, 1_200_000, 700_041),
            chunk(0x43, 400_000, 400_041),
        ]),
    );
    p.mtime = FileTime::new(-86_400, 5).expect("a time");
    p.mode = 0o755;
    p.blake3 = [0x12; 32];
    p.transform = Some(TransformRef {
        id: 7,
        out_len: FileSize::new(2_600_000),
        params,
    });
    p.previews = vec![
        Preview {
            kind: 1,
            w: 256,
            h: 192,
            chunks: vec![chunk(0x61, 20_000, 20_041)],
        },
        Preview {
            kind: 2,
            w: 1024,
            h: 768,
            chunks: vec![chunk(0x62, 90_000, 60_041), chunk(0x63, 5_000, 5_041)],
        },
    ];
    p.streams = vec![
        StreamRef {
            kind: 1,
            w: 1280,
            h: 720,
            bandwidth: 2_500_000,
            chunks: vec![chunk(0x64, 300_000, 300_041), chunk(0x65, 200_000, 200_041)],
        },
        StreamRef {
            kind: 2,
            w: 0,
            h: 0,
            bandwidth: 128_000,
            chunks: vec![chunk(0x66, 50_000, 50_041)],
        },
    ];
    p.xattrs = vec![
        ("user.comment".to_string(), b"hello".to_vec()),
        (
            "com.apple.quarantine".to_string(),
            vec![0x00, 0x01, 0x02, 0xFF],
        ),
    ];
    p.labels = vec!["red".to_string(), "invoice".to_string()];
    p.derived = vec![
        DerivedRef {
            extractor: "ocr".to_string(),
            model_id: "tesseract".to_string(),
            model_version: "5.3.1".to_string(),
            record: chunk(0x71, 4_000, 4_041),
        },
        DerivedRef {
            extractor: "exif".to_string(),
            model_id: String::new(),
            model_version: String::new(),
            record: chunk(0x72, 600, 641),
        },
    ];
    p.created_at = UnixTime::from_secs(1_700_000_200);
    p
}

/// `full()` as a newer minor version writes it: a field 40 nobody here knows (not in `crit`), and a
/// `crit` list that names the known key 7.
pub fn full_with_unknown() -> PointerV2 {
    let mut p = full();
    p.unknown = UnknownFields::from_entries(vec![
        (40, Value::Text("from a newer minor version".to_string())),
        (
            41,
            Value::Array(vec![Value::Uint(1), Value::Bytes(vec![9, 9, 9])]),
        ),
    ])
    .expect("ascending unknown keys");
    p.crit = vec![7];
    p
}

/// The pointer of a huge file: 1,025 chunks, in a sealed object; the object and the reference come from
/// `sealer`.
pub fn huge(sealer: &dyn ObjectSealer) -> (PointerV2, Vec<u8>, Vec<ChunkRef>) {
    let refs = huge_chunks(1025);
    let (sealed, list) = azlin_content::pointer::encode_chunk_list(&refs, &space(), sealer)
        .expect("the list encodes");
    let p = base(list.total_raw.get(), ChunkList::Object(list));
    (p, sealed, refs)
}

/// Signs `p` as the author for the test space.
pub fn signed(mut p: PointerV2) -> PointerV2 {
    p.sign(&space(), &author_key()).expect("signs");
    p
}

/// A copy of `p` with a signature the test chose (no check that it verifies).
pub fn with_sig(p: &PointerV2, sig: [u8; 64]) -> PointerV2 {
    let mut q = p.duplicate();
    q.sig = Some(Signature::from_bytes(sig));
    q
}

// ---------------------------------------------------------------------------------------------
// The sealer double
// ---------------------------------------------------------------------------------------------

/// A sealer that authenticates and does not encrypt (the contract of `ObjectSealer` is about what opens,
/// not about secrecy): `"TSEAL001" || tag (32) || plaintext`, the tag a keyed BLAKE3 of the context's
/// length, the context and the plaintext. It counts its calls and keeps the contexts it saw.
pub struct TestSealer {
    pub key: [u8; 32],
    pub seals: Cell<usize>,
    pub opens: Cell<usize>,
    pub contexts: RefCell<Vec<Vec<u8>>>,
}

pub const SEAL_MAGIC: &[u8; 8] = b"TSEAL001";

impl Default for TestSealer {
    fn default() -> TestSealer {
        TestSealer::new()
    }
}

impl TestSealer {
    pub fn new() -> TestSealer {
        TestSealer::with_key([0x5E; 32])
    }

    pub fn with_key(key: [u8; 32]) -> TestSealer {
        TestSealer {
            key,
            seals: Cell::new(0),
            opens: Cell::new(0),
            contexts: RefCell::new(Vec::new()),
        }
    }

    fn tag(&self, context: &[u8], plaintext: &[u8]) -> [u8; 32] {
        let mut h = blake3::Hasher::new_keyed(&self.key);
        h.update(&(context.len() as u64).to_le_bytes());
        h.update(context);
        h.update(plaintext);
        *h.finalize().as_bytes()
    }
}

impl ObjectSealer for TestSealer {
    fn seal(&self, context: &[u8], plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        self.seals.set(self.seals.get() + 1);
        self.contexts.borrow_mut().push(context.to_vec());
        let mut out = Vec::with_capacity(40 + plaintext.len());
        out.extend_from_slice(SEAL_MAGIC);
        out.extend_from_slice(&self.tag(context, plaintext));
        out.extend_from_slice(plaintext);
        Ok(out)
    }

    fn open(&self, context: &[u8], sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        self.opens.set(self.opens.get() + 1);
        let rest = sealed.strip_prefix(SEAL_MAGIC).ok_or(SealError::Auth)?;
        if rest.len() < 32 {
            return Err(SealError::Auth);
        }
        let (tag, plaintext) = rest.split_at(32);
        if tag != self.tag(context, plaintext) {
            return Err(SealError::Auth);
        }
        Ok(Zeroizing::new(plaintext.to_vec()))
    }
}

/// A sealer that refuses everything with `error`.
pub struct RefusingSealer(pub SealError);

impl ObjectSealer for RefusingSealer {
    fn seal(&self, _context: &[u8], _plaintext: &[u8]) -> Result<Vec<u8>, SealError> {
        Err(self.0)
    }

    fn open(&self, _context: &[u8], _sealed: &[u8]) -> Result<Zeroizing<Vec<u8>>, SealError> {
        Err(self.0)
    }
}

// ---------------------------------------------------------------------------------------------
// Editing at the level of the CBOR value
// ---------------------------------------------------------------------------------------------

/// The tree of some bytes.
pub fn tree(bytes: &[u8]) -> Value {
    decode(bytes, &OBJECT).expect("the bytes are CBOR")
}

/// The bytes of a tree.
pub fn bytes_of(v: &Value) -> Vec<u8> {
    encode(v, &OBJECT).expect("the tree encodes")
}

/// `bytes` with the value of `key` set (or removed, for `None`).
pub fn with_entry(bytes: &[u8], key: u64, value: Option<Value>) -> Vec<u8> {
    let Value::Map(map) = tree(bytes) else {
        panic!("not a map");
    };
    let mut entries: Vec<(Value, Value)> = map
        .into_entries()
        .into_iter()
        .filter(|(k, _)| *k != Value::Uint(key))
        .collect();
    if let Some(v) = value {
        entries.push((Value::Uint(key), v));
    }
    bytes_of(&Value::Map(Map::from_entries(entries).expect("a map")))
}

/// The value of `key` in the map of `bytes`.
pub fn entry(bytes: &[u8], key: u64) -> Value {
    let Value::Map(map) = tree(bytes) else {
        panic!("not a map");
    };
    map.get(&Value::Uint(key))
        .unwrap_or_else(|| panic!("no key {key}"))
        .clone()
}

/// The paths (indices into arrays and maps) of every leaf of `v`.
pub fn leaf_paths(v: &Value) -> Vec<Vec<usize>> {
    fn walk(v: &Value, path: &mut Vec<usize>, out: &mut Vec<Vec<usize>>) {
        match v {
            Value::Array(items) => {
                for (i, item) in items.iter().enumerate() {
                    path.push(i);
                    walk(item, path, out);
                    path.pop();
                }
            }
            Value::Map(map) => {
                for (i, (_, value)) in map.iter().enumerate() {
                    path.push(i);
                    walk(value, path, out);
                    path.pop();
                }
            }
            _ => out.push(path.clone()),
        }
    }
    let mut out = Vec::new();
    walk(v, &mut Vec::new(), &mut out);
    out
}

/// The value at `path`.
pub fn value_at<'v>(v: &'v Value, path: &[usize]) -> &'v Value {
    match path.split_first() {
        None => v,
        Some((i, rest)) => match v {
            Value::Array(items) => value_at(&items[*i], rest),
            Value::Map(map) => value_at(&map.iter().nth(*i).expect("an entry").1, rest),
            _ => panic!("no such path"),
        },
    }
}

/// `v` with the value at `path` replaced.
pub fn replace_at(v: &Value, path: &[usize], new: &Value) -> Value {
    match path.split_first() {
        None => new.clone(),
        Some((i, rest)) => match v {
            Value::Array(items) => {
                let mut items = items.clone();
                items[*i] = replace_at(&items[*i], rest, new);
                Value::Array(items)
            }
            Value::Map(map) => {
                let mut entries: Vec<(Value, Value)> = map.iter().cloned().collect();
                entries[*i].1 = replace_at(&entries[*i].1, rest, new);
                Value::Map(Map::from_entries(entries).expect("a map"))
            }
            _ => panic!("no such path"),
        },
    }
}

/// A value of another type than `v`: text for everything that is not text, an integer for text.
pub fn wrong_type(v: &Value) -> Value {
    match v {
        Value::Text(_) => Value::Uint(0),
        _ => Value::Text("x".to_string()),
    }
}

// ---------------------------------------------------------------------------------------------
// Random valid pointers
// ---------------------------------------------------------------------------------------------

fn random_chunk(rng: &mut SplitMix64) -> ChunkRef {
    let raw = 1 + rng.below(4 * 1024 * 1024) as u32;
    let stored = if rng.chance(50) {
        raw + 41
    } else {
        42 + rng.below(u64::from(raw)) as u32
    };
    ChunkRef {
        full: Full::from_bytes(rng.array::<32>()),
        raw_len: ChunkLen::new(raw).expect("a length"),
        pack: PackId::from_bytes(rng.array::<16>()),
        offset: PackOffset::new(rng.below(4 * 1024 * 1024) as u32),
        stored_len: ChunkLen::new(stored).expect("a length"),
    }
}

fn random_chunks(rng: &mut SplitMix64, n: u64) -> Vec<ChunkRef> {
    let mut v = Vec::with_capacity(n as usize);
    for _ in 0..n {
        v.push(random_chunk(rng));
    }
    v
}

fn random_text(rng: &mut SplitMix64, max: u64) -> String {
    let n = 1 + rng.below(max);
    (0..n)
        .map(|_| char::from(b'a' + rng.below(26) as u8))
        .collect()
}

fn random_value(rng: &mut SplitMix64, depth: u32) -> Value {
    match rng.below(if depth >= 2 { 4 } else { 6 }) {
        0 => Value::Uint(rng.next_u64()),
        1 => Value::Nint(rng.below(1 << 40)),
        2 => {
            let n = rng.below(40) as usize;
            Value::Bytes(rng.bytes(n))
        }
        3 => Value::Text(random_text(rng, 20)),
        4 => Value::Array(
            (0..rng.below(4))
                .map(|_| random_value(rng, depth + 1))
                .collect(),
        ),
        _ => Value::Map(
            Map::from_entries(
                (0..rng.below(4))
                    .map(|i| (Value::Uint(i), random_value(rng, depth + 1)))
                    .collect(),
            )
            .expect("a map"),
        ),
    }
}

/// A random pointer that satisfies every rule (so `to_cbor` writes it): any number of chunks from none
/// to a chunk-list object, optional transform, previews, streams, attributes, labels, derived records,
/// unknown fields, `crit` and a signature (random bytes: nothing here verifies it).
pub fn random_pointer(rng: &mut SplitMix64) -> PointerV2 {
    let shape = rng.below(20);
    let n: u64 = match shape {
        0 => 0,
        1 => 1,
        2..=11 => 2 + rng.below(6),
        12 => 1024,
        13 => 1025 + rng.below(200),
        _ => 1 + rng.below(40),
    };
    let (chunks, summed) = if n > 1024 {
        // an object: the name is random, the total is any possible total for that many chunks
        let count = n as u32;
        let low = n;
        let high = n * 4 * 1024 * 1024;
        let total = low + rng.below(high - low + 1);
        (
            ChunkList::Object(ChunkListRef {
                name: rng.array::<32>(),
                count,
                total_raw: FileSize::new(total),
            }),
            total,
        )
    } else {
        let refs = random_chunks(rng, n);
        let summed = total_raw(&refs);
        (ChunkList::Inline(refs), summed)
    };
    let transform = if rng.chance(25) {
        Some(TransformRef {
            id: rng.below(65_536) as u16,
            out_len: FileSize::new(summed),
            params: random_value(rng, 0),
        })
    } else {
        None
    };
    let size = if transform.is_some() {
        rng.below(1 << 50)
    } else {
        summed
    };
    let mut p = PointerV2 {
        size: FileSize::new(size),
        mtime: FileTime::new(
            -62_167_219_200 + rng.below(253_402_300_799 + 62_167_219_200 + 1) as i64,
            rng.below(1_000_000_000) as u32,
        )
        .expect("a time"),
        mode: rng.below(0o7777 + 1) as u32,
        blake3: rng.array::<32>(),
        transform,
        chunks,
        author_device: DeviceId::from_bytes(rng.array::<16>()),
        sig: if rng.chance(70) {
            Some(Signature::from_bytes(rng.array::<64>()))
        } else {
            None
        },
        previews: Vec::new(),
        streams: Vec::new(),
        xattrs: Vec::new(),
        labels: Vec::new(),
        derived: Vec::new(),
        created_at: UnixTime::from_secs(rng.below(253_402_300_799 + 1)),
        unknown: UnknownFields::none(),
        crit: Vec::new(),
    };
    if rng.chance(30) {
        for _ in 0..1 + rng.below(3) {
            let m = 1 + rng.below(3);
            p.previews.push(Preview {
                kind: rng.below(256) as u8,
                w: rng.below(65_536) as u32,
                h: rng.below(65_536) as u32,
                chunks: random_chunks(rng, m),
            });
        }
    }
    if rng.chance(20) {
        for _ in 0..1 + rng.below(2) {
            let m = 1 + rng.below(5);
            p.streams.push(StreamRef {
                kind: rng.below(256) as u8,
                w: rng.below(65_536) as u32,
                h: rng.below(65_536) as u32,
                bandwidth: rng.next_u64() as u32,
                chunks: random_chunks(rng, m),
            });
        }
    }
    if rng.chance(30) {
        for i in 0..1 + rng.below(4) {
            let name = format!("user.{}.{i}", random_text(rng, 10));
            let value_len = rng.below(100) as usize;
            p.xattrs.push((name, rng.bytes(value_len)));
        }
    }
    if rng.chance(30) {
        for _ in 0..1 + rng.below(3) {
            p.labels.push(random_text(rng, 30));
        }
    }
    if rng.chance(25) {
        for _ in 0..1 + rng.below(3) {
            p.derived.push(DerivedRef {
                extractor: random_text(rng, 12),
                model_id: if rng.chance(50) {
                    random_text(rng, 12)
                } else {
                    String::new()
                },
                model_version: if rng.chance(50) {
                    random_text(rng, 8)
                } else {
                    String::new()
                },
                record: random_chunk(rng),
            });
        }
    }
    if rng.chance(20) {
        let mut entries = Vec::new();
        let mut key = 16 + rng.below(30);
        for _ in 0..1 + rng.below(3) {
            entries.push((key, random_value(rng, 0)));
            key += 1 + rng.below(100);
        }
        p.unknown = UnknownFields::from_entries(entries).expect("ascending keys");
    }
    if rng.chance(15) {
        let mut crit = Vec::new();
        for key in 1..=15u64 {
            if rng.chance(25) {
                crit.push(key);
            }
        }
        p.crit = crit;
    }
    p
}
