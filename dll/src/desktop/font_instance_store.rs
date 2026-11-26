//! The static instances of variable fonts, kept between runs.
//!
//! A variable system font (macOS's `SFNS.ttf`, Roboto and Cantarell on others) is drawn as a static INSTANCE of
//! it at the coordinates a style asks (`layout/src/solver3/getters.rs`, `select_variable_instances`). Making one
//! applies every glyph's variation data: about 100 ms for `SFNS.ttf`, on every start, in the first layout, on the
//! way to the first frame. An instance is a pure function of the font file and the coordinates, so the bytes of
//! one are kept in a directory of the app's cache and read back (one read, a checksum) at the next start.
//!
//! The store holds no state but its directory and its limits. It is a SET-ASSOCIATIVE cache of files:
//!
//! - an instance goes to the set its key hashes to (`fi-<set>-<way>.bin`, `sets` x `ways` files at most);
//! - a set that is full gives its OLDEST file (by modification time) to the new instance: the file is replaced
//!   by a rename, so the cache is bounded in files and in bytes (`sets * ways * max_entry_bytes`) and nothing
//!   is ever deleted;
//! - a file is written to a temporary name of its slot (one per slot, created exclusively, so concurrent
//!   writers do not interleave) and renamed over its place: a reader sees the old file or the new one;
//! - a file is VALIDATED on load: its magic and version, the key it echoes, its length, a checksum of the
//!   bytes and the table directory of the font (a cut file lists tables beyond its end). Anything that does
//!   not hold is no entry: the instance is made again and the file replaced.
//!
//! The directory is the caller's (`App::create` puts it next to the font manifest).

use std::{
    fs::{File, OpenOptions},
    io::{Read, Write},
    path::{Path, PathBuf},
    time::{Duration, SystemTime},
};

/// The format of a file: bumped when the layout of a file or what an instance is made of changes (the
/// instancer is part of the key's meaning: an `allsorts` that makes other bytes for the same coordinates is a
/// new format).
pub const FORMAT_VERSION: u32 = 1;

const MAGIC: [u8; 4] = *b"AZFI";

/// What identifies an instance: the font file (its length and a hash of its content), the face in it and the
/// axis values in 1/64 units, one per axis in `fvar` order.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct InstanceKey {
    /// Length of the font file in bytes.
    pub font_len: u64,
    /// Hash of the content of the font file.
    pub font_hash: u64,
    /// Index of the face in the file.
    pub font_index: u32,
    /// The axis values, 1/64 units, in `fvar` order.
    pub cells: Vec<i32>,
}

/// The directory of kept instances and the limits that bound it.
#[derive(Debug, Clone)]
pub struct FontInstanceStore {
    dir: PathBuf,
    sets: usize,
    ways: usize,
    max_entry_bytes: u64,
}

/// How long a temporary file of a slot is left alone: a writer that has not finished by then is dead or stuck,
/// and the slot's next writer takes the file over.
const TEMPORARY_FILE_TTL: Duration = Duration::from_secs(60);

impl FontInstanceStore {
    /// A store in `dir` with the default limits: 16 sets of 2 files, 8 MiB each (at most 256 MiB, 32 files).
    #[must_use]
    pub fn new(dir: impl Into<PathBuf>) -> Self {
        Self::with_limits(dir, 16, 2, 8 << 20)
    }

    /// A store in `dir` of `sets` x `ways` files of at most `max_entry_bytes` bytes of font each (an
    /// instance bigger than that is not kept).
    #[must_use]
    pub fn with_limits(dir: impl Into<PathBuf>, sets: usize, ways: usize, max_entry_bytes: u64) -> Self {
        Self {
            dir: dir.into(),
            sets: sets.max(1),
            ways: ways.max(1),
            max_entry_bytes,
        }
    }

    /// The directory the files are in.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// The instance kept for `key`, if a file holds it intact.
    #[must_use]
    pub fn load(&self, key: &InstanceKey) -> Option<Vec<u8>> {
        let set = self.set_of(key);
        for way in 0..self.ways {
            let path = self.slot_path(set, way);
            let Some(bytes) = read_entry(&path, key) else {
                continue;
            };
            // The use counts as recent: the set replaces its least recently used file.
            if let Ok(file) = OpenOptions::new().write(true).open(&path) {
                let _ = file.set_modified(SystemTime::now());
            }
            return Some(bytes);
        }
        None
    }

    /// Keeps `bytes` as the instance for `key`: true when a file now holds it. An instance that is not a
    /// complete font, or is bigger than the limit, is not kept; a store that cannot write (no directory, no
    /// permission, a writer of the same slot at work) keeps nothing and says false.
    pub fn store(&self, key: &InstanceKey, bytes: &[u8]) -> bool {
        if bytes.len() as u64 > self.max_entry_bytes || !tables_within(bytes) {
            return false;
        }
        if std::fs::create_dir_all(&self.dir).is_err() {
            return false;
        }
        let set = self.set_of(key);
        let way = self.way_to_fill(set, key);
        let target = self.slot_path(set, way);
        let temporary = self.temporary_path(set, way);
        if !write_entry(&temporary, key, bytes) {
            return false;
        }
        if std::fs::rename(&temporary, &target).is_err() {
            return false;
        }
        true
    }

    /// The instance for `key`: the kept one, or what `make` makes (kept for the next start). `make` runs only
    /// when no intact file holds the instance.
    pub fn get_or_make(&self, key: &InstanceKey, make: impl FnOnce() -> Option<Vec<u8>>) -> Option<Vec<u8>> {
        if let Some(kept) = self.load(key) {
            return Some(kept);
        }
        let made = make()?;
        self.store(key, &made);
        Some(made)
    }

    /// The set a key hashes to.
    fn set_of(&self, key: &InstanceKey) -> usize {
        (key_hash(key) % self.sets as u64) as usize
    }

    fn slot_path(&self, set: usize, way: usize) -> PathBuf {
        self.dir.join(format!("fi-{set:02x}-{way}.bin"))
    }

    fn temporary_path(&self, set: usize, way: usize) -> PathBuf {
        self.dir.join(format!("fi-{set:02x}-{way}.tmp"))
    }

    /// The way of `set` a new file for `key` takes: the one that holds `key` already, else one whose file is
    /// damaged (it is no entry for anything), else an empty one, else the least recently used.
    fn way_to_fill(&self, set: usize, key: &InstanceKey) -> usize {
        let mut damaged: Option<usize> = None;
        let mut empty: Option<usize> = None;
        let mut oldest: Option<(usize, SystemTime)> = None;
        for way in 0..self.ways {
            let path = self.slot_path(set, way);
            let Ok(meta) = std::fs::metadata(&path) else {
                empty = empty.or(Some(way));
                continue;
            };
            match read_key(&path) {
                Some(held) if &held == key => return way, // the same instance again: replace it
                Some(_) => {
                    let modified = meta.modified().unwrap_or(SystemTime::UNIX_EPOCH);
                    if oldest.map_or(true, |(_, at)| modified < at) {
                        oldest = Some((way, modified));
                    }
                }
                None => damaged = damaged.or(Some(way)),
            }
        }
        damaged
            .or(empty)
            .or(oldest.map(|(way, _)| way))
            .unwrap_or(0)
    }
}

/// The bytes of the header of a file for `key`, then the font follows:
/// magic, version, font length, font hash, face index, cell count, the cells, payload length, checksum.
fn header_of(key: &InstanceKey, payload: &[u8]) -> Vec<u8> {
    let mut header = Vec::with_capacity(48 + key.cells.len() * 4);
    header.extend_from_slice(&MAGIC);
    header.extend_from_slice(&FORMAT_VERSION.to_le_bytes());
    header.extend_from_slice(&key.font_len.to_le_bytes());
    header.extend_from_slice(&key.font_hash.to_le_bytes());
    header.extend_from_slice(&key.font_index.to_le_bytes());
    header.extend_from_slice(&(key.cells.len() as u32).to_le_bytes());
    for cell in &key.cells {
        header.extend_from_slice(&cell.to_le_bytes());
    }
    header.extend_from_slice(&(payload.len() as u64).to_le_bytes());
    header.extend_from_slice(&checksum(payload).to_le_bytes());
    header
}

/// Writes the file for `key` at `path` (exclusively: `false` when another writer holds the slot).
fn write_entry(path: &Path, key: &InstanceKey, payload: &[u8]) -> bool {
    let open = |exclusive: bool| {
        let mut options = OpenOptions::new();
        options.write(true);
        if exclusive {
            options.create_new(true);
        } else {
            options.create(true).truncate(true);
        }
        options.open(path)
    };
    let mut file = match open(true) {
        Ok(file) => file,
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
            // A writer of this slot is at work, or died: its file is taken over when it is old.
            let stale = std::fs::metadata(path)
                .and_then(|meta| meta.modified())
                .ok()
                .and_then(|at| SystemTime::now().duration_since(at).ok())
                .is_some_and(|age| age > TEMPORARY_FILE_TTL);
            if !stale {
                return false;
            }
            match open(false) {
                Ok(file) => file,
                Err(_) => return false,
            }
        }
        Err(_) => return false,
    };
    let header = header_of(key, payload);
    file.write_all(&header).and_then(|()| file.write_all(payload)).is_ok()
}

/// The key a file echoes, from its header alone.
fn read_key(path: &Path) -> Option<InstanceKey> {
    let mut file = File::open(path).ok()?;
    read_header(&mut file).map(|header| header.key)
}

struct Header {
    key: InstanceKey,
    payload_len: u64,
    checksum: u64,
}

fn read_header(file: &mut File) -> Option<Header> {
    let mut fixed = [0u8; 4 + 4 + 8 + 8 + 4 + 4];
    file.read_exact(&mut fixed).ok()?;
    if fixed[0..4] != MAGIC {
        return None;
    }
    if u32::from_le_bytes(fixed[4..8].try_into().ok()?) != FORMAT_VERSION {
        return None;
    }
    let font_len = u64::from_le_bytes(fixed[8..16].try_into().ok()?);
    let font_hash = u64::from_le_bytes(fixed[16..24].try_into().ok()?);
    let font_index = u32::from_le_bytes(fixed[24..28].try_into().ok()?);
    let cell_count = u32::from_le_bytes(fixed[28..32].try_into().ok()?) as usize;
    if cell_count > 64 {
        return None; // no font has so many axes
    }
    let mut cells = Vec::with_capacity(cell_count);
    for _ in 0..cell_count {
        let mut cell = [0u8; 4];
        file.read_exact(&mut cell).ok()?;
        cells.push(i32::from_le_bytes(cell));
    }
    let mut tail = [0u8; 16];
    file.read_exact(&mut tail).ok()?;
    Some(Header {
        key: InstanceKey {
            font_len,
            font_hash,
            font_index,
            cells,
        },
        payload_len: u64::from_le_bytes(tail[0..8].try_into().ok()?),
        checksum: u64::from_le_bytes(tail[8..16].try_into().ok()?),
    })
}

/// The font a file holds for `key`, when the file is intact: its header echoes `key`, its length is the one the
/// header says, its bytes have the checksum the header says and its table directory lies within them.
fn read_entry(path: &Path, key: &InstanceKey) -> Option<Vec<u8>> {
    let mut file = File::open(path).ok()?;
    let header = read_header(&mut file)?;
    if &header.key != key {
        return None;
    }
    let on_disk = file.metadata().ok()?.len();
    let header_len = 4 + 4 + 8 + 8 + 4 + 4 + key.cells.len() as u64 * 4 + 8 + 8;
    if on_disk.checked_sub(header_len) != Some(header.payload_len) {
        return None;
    }
    let mut payload = Vec::with_capacity(usize::try_from(header.payload_len).ok()?);
    file.read_to_end(&mut payload).ok()?;
    if payload.len() as u64 != header.payload_len || checksum(&payload) != header.checksum || !tables_within(&payload) {
        return None;
    }
    Some(payload)
}

/// A 64-bit hash of the bytes: four lanes of multiply and rotate over 32-byte blocks (fast enough for a
/// font of several MB in a millisecond or two), the tail and the length folded in. It guards a file against
/// damage, not against an attacker: the cache is the user's own directory.
#[must_use]
pub fn checksum(bytes: &[u8]) -> u64 {
    const P1: u64 = 0x9E37_79B1_85EB_CA87;
    const P2: u64 = 0xC2B2_AE3D_27D4_EB4F;
    let mut lanes = [P1.wrapping_add(P2), P2, 0, P1.wrapping_neg()];
    let mut blocks = bytes.chunks_exact(32);
    for block in &mut blocks {
        for (lane, word) in lanes.iter_mut().zip(block.chunks_exact(8)) {
            let word = u64::from_le_bytes(word.try_into().unwrap_or([0; 8]));
            *lane = (*lane).wrapping_add(word.wrapping_mul(P2)).rotate_left(31).wrapping_mul(P1);
        }
    }
    let mut hash = lanes[0]
        .rotate_left(1)
        .wrapping_add(lanes[1].rotate_left(7))
        .wrapping_add(lanes[2].rotate_left(12))
        .wrapping_add(lanes[3].rotate_left(18));
    for lane in lanes {
        hash = (hash ^ lane.wrapping_mul(P2).rotate_left(31).wrapping_mul(P1)).wrapping_mul(P1).wrapping_add(P2);
    }
    for byte in blocks.remainder() {
        hash = (hash ^ u64::from(*byte).wrapping_mul(P1)).rotate_left(11).wrapping_mul(P2);
    }
    hash ^= bytes.len() as u64;
    hash ^= hash >> 33;
    hash = hash.wrapping_mul(P2);
    hash ^= hash >> 29;
    hash
}

/// The hash a key picks its set by (stable across runs and platforms: it names files).
fn key_hash(key: &InstanceKey) -> u64 {
    let mut bytes = Vec::with_capacity(24 + key.cells.len() * 4);
    bytes.extend_from_slice(&key.font_len.to_le_bytes());
    bytes.extend_from_slice(&key.font_hash.to_le_bytes());
    bytes.extend_from_slice(&key.font_index.to_le_bytes());
    for cell in &key.cells {
        bytes.extend_from_slice(&cell.to_le_bytes());
    }
    checksum(&bytes)
}

/// Whether every table the directory of the sfnt `font` lists lies within its bytes (a cut file does not).
#[must_use]
pub fn tables_within(font: &[u8]) -> bool {
    if font.len() < 12 {
        return false;
    }
    let tables = usize::from(u16::from_be_bytes([font[4], font[5]]));
    (0..tables).all(|i| {
        font.get(12 + i * 16 + 8..12 + i * 16 + 16).is_some_and(|record| {
            let offset = u32::from_be_bytes([record[0], record[1], record[2], record[3]]) as usize;
            let length = u32::from_be_bytes([record[4], record[5], record[6], record[7]]) as usize;
            offset.checked_add(length).is_some_and(|end| end <= font.len())
        })
    })
}

#[cfg(test)]
#[path = "font_instance_store_test.rs"]
mod font_instance_store_test;
