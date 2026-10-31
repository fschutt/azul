//! Small files packed together on an encrypted drive (feature `encryption`).
//!
//! An Azlin server codes every object in chunks of 4 MiB, each cut into 4 data shards and 2
//! parity shards on 6 disks. A 10 KB file as an object of its own becomes six shards of about
//! 2.5 KB, and reading it costs four seeks on four disks. Packed with other small files into
//! one object of up to 4 MiB, it is one small ranged read that lands in ONE data shard: the code
//! is systematic, so a data shard is a plain slice of its chunk.
//!
//! - **What is packed:** files under [`PackOptions::threshold`] written to an encrypted drive
//!   that packs ([`EncryptedDrive::with_packing`]). Plain buckets never pack, so their files
//!   stay readable by any S3 tool; a drive that does not pack still reads packed files.
//! - **A pack** is an AZL1 object with its own file key, wrapped by the drive key like any
//!   object's. Its segments are 4 KiB and never compressed: each file is compressed on its own
//!   before it goes in, and a stored segment's place in the object follows from its index. The
//!   index keeps, per file version, the pack, the file's record in it and the file's own BLAKE3
//!   ([`PackedAt`]), so reading a file is one ranged GET of its segments
//!   ([`crate::crypto::azl1::StoredSegments`]).
//! - **Writes:** a batch ([`crate::Drive::put_many`], what a sync sends) fills packs of its own
//!   and commits each pack in one index change. Single writes meet in an open pack: its first
//!   writer seals it at once when it is alone, and waits up to [`PackOptions::linger`] for the
//!   others when writers overlap, so a sync stays responsive and parallel writers share a pack.
//! - **Crash safety:** a pack goes into the journal before its upload and leaves it after its
//!   index commit. The index never names a pack that is not in the bucket yet, so a crash in
//!   between leaves an object nothing names, which [`EncryptedDrive::recover_packs`] deletes.
//! - **Never rewritten:** an update goes into a new pack, and the old version's record becomes
//!   garbage in its own. [`EncryptedDrive::repack`] copies the live files of packs that fell
//!   below [`PackOptions::repack_below_percent`] into new packs, one index change per new pack,
//!   and retires the old packs; [`EncryptedDrive::collect_retired`] deletes them after
//!   [`PackOptions::retired_grace_secs`], unless the index names them again (a restore).
//! - **Reads:** a ranged read cut short asks for the rest; a pack read [`HOT_READS`] times is
//!   fetched whole once and kept as ciphertext ([`HOT_PACKS`] of them).
//! - **Shares:** a pack's key opens every file in it, so a shared packed file first moves into an
//!   object of its own.
//! - **Quota and dedup:** the quota counts a pack's stored bytes once, padding and garbage
//!   included; nothing is deduplicated.

use std::{
    collections::{BTreeMap, HashMap, HashSet, VecDeque},
    fs,
    path::{Path, PathBuf},
    sync::{
        atomic::{AtomicU64, Ordering},
        Arc, Condvar, Mutex, MutexGuard, PoisonError,
    },
    time::{Duration, Instant},
};

use azlin_compress::Options;
use serde::{Deserialize, Serialize};
use zeroize::Zeroizing;

pub use crate::shards::{data_shard_len, data_shards_of, CHUNK_SIZE, DATA_SHARDS};
use crate::{
    crypto::{
        azl1::{self, PublicHeader, StoredSegments, WriteOptions, HEADER_LEN, PREFIX_LEN, SEGMENT_OVERHEAD},
        codec::{self, Compression},
        CryptoError, DriveKey, ObjectId, WrappedKey,
    },
    encrypted::{
        expect_of, info_of, read_error, EncryptedDrive, Expect, IndexChange, IndexEntry, ReadError,
        StoredObject,
    },
    key::check_key,
    local::write_atomically,
    time::now_unix,
    BatchPut, ByteRange, Drive, DriveError, ObjectInfo,
};

/// Files below this many bytes are packed ([`crate::SMALL_FILE_BYTES`] says why).
pub const PACK_THRESHOLD: u64 = crate::SMALL_FILE_BYTES;
/// The largest pack: one chunk, so a pack is coded as one stripe of four data shards.
pub const PACK_MAX: u64 = CHUNK_SIZE;
/// The record bytes an open pack takes before it counts as full: a sixteenth is left for the
/// padding that keeps every file inside one data shard.
pub const PACK_FILL: u64 = PACK_MAX - PACK_MAX / 16;
/// Plaintext bytes per segment of a pack: a 10 KB file reads at most four segments, and the 17
/// bytes of codec byte and tag per segment cost 0.4 %.
pub const PACK_SEGMENT: u32 = 4096;
/// Packs from this many bytes on place their files so that none crosses a data shard. A smaller
/// pack's shards are not much bigger than its segments, so aligning would cost more padding
/// than it saves; its files go in one after the other, and repacking merges small packs.
pub const ALIGN_FROM: u64 = 1 << 20;
/// How long the first writer of an open pack waits for the others while writers overlap.
pub const DEFAULT_LINGER_MS: u64 = 50;
/// A pack is repacked when its live files hold less than this percentage of its plaintext.
pub const REPACK_BELOW_PERCENT: u64 = 50;
/// Packs smaller than this are merged when there are two or more of them (packs a lone writer
/// sealed, the rest of a batch).
pub const SMALL_PACK: u64 = 1 << 20;
/// How long a retired pack stays in the bucket: a version restored from the drive's history
/// within a month still finds its bytes.
pub const RETIRED_GRACE_SECS: u64 = 30 * 86_400;
/// A pack in the journal for longer than this is no longer in flight: its writer stopped.
pub const JOURNAL_STALE_SECS: u64 = 15 * 60;
/// Reads of one pack after which it is fetched whole and kept (a sync reading a folder).
pub const HOT_READS: u32 = 3;
/// Whole packs kept in memory, as ciphertext: at most 4 MiB each.
pub const HOT_PACKS: usize = 4;
/// How often a ranged read that was cut short (or got no answer) asks for the rest.
pub const RESUME_ATTEMPTS: u32 = 3;

/// The journal's file of a pack in flight.
const IN_FLIGHT_FORMAT: &str = "azul-storage pack in flight 1";
/// The journal's list of retired packs.
const RETIRED_FORMAT: &str = "azul-storage retired packs 1";
/// The folder of the packs in flight, in the journal folder.
const IN_FLIGHT_DIR: &str = "in-flight";
/// The list of retired packs, in the journal folder.
const RETIRED_FILE: &str = "retired.json";
/// The packs this device knows of, in the journal folder: one line `+<id> <bytes>` per pack it
/// wrote or saw named, appended; written anew without the packs that left the bucket.
const KNOWN_FILE: &str = "known.log";
/// Packs whose reads are counted, at most; the count starts again beyond it.
const COUNTED_PACKS: usize = 1024;

/// How an encrypted drive packs small files ([`EncryptedDrive::with_packing`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackOptions {
    /// Files below this many bytes are packed ([`PACK_THRESHOLD`]).
    pub threshold: u64,
    /// The folder of the packs in flight and of the retired packs, in the drive's cache folder.
    /// `None` keeps both in memory: a crash then leaves its pack to the bucket's reconcile.
    pub journal: Option<PathBuf>,
    /// How long the first writer of an open pack waits for the others while writers overlap
    /// ([`DEFAULT_LINGER_MS`]); a writer alone never waits.
    pub linger: Duration,
    /// A pack whose live files hold less than this percentage of it is repacked
    /// ([`REPACK_BELOW_PERCENT`]).
    pub repack_below_percent: u64,
    /// How long a retired pack stays before it is deleted ([`RETIRED_GRACE_SECS`]).
    pub retired_grace_secs: u64,
}

impl Default for PackOptions {
    fn default() -> Self {
        PackOptions {
            threshold: PACK_THRESHOLD,
            journal: None,
            linger: Duration::from_millis(DEFAULT_LINGER_MS),
            repack_below_percent: REPACK_BELOW_PERCENT,
            retired_grace_secs: RETIRED_GRACE_SECS,
        }
    }
}

impl PackOptions {
    /// The journal in `dir` (made when the first pack goes up).
    #[must_use]
    pub fn with_journal(mut self, dir: PathBuf) -> Self {
        self.journal = Some(dir);
        self
    }
}

/// A pack as its files' index entries name it: what opens its segments without its header.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackRef {
    /// The pack's object (`data/..`).
    pub id: ObjectId,
    /// Its plaintext bytes: the records and the padding between them.
    pub plaintext_size: u64,
    /// Its bytes in the bucket: what the quota counts, once per pack.
    pub object_len: u64,
    /// Plaintext bytes per segment ([`PACK_SEGMENT`] when this build wrote it).
    pub segment_size: u32,
    /// The nonce prefix of its segments; it is in the pack's public header too.
    pub prefix: [u8; PREFIX_LEN],
}

/// Where a packed file version lies: its pack, and its record in the pack's plaintext (the codec
/// byte, then the file, compressed when that paid).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PackedAt {
    /// The pack the record is in.
    pub pack: PackRef,
    /// Where the record starts in the pack's plaintext.
    pub offset: u64,
    /// The record's bytes.
    pub length: u64,
}

impl PackedAt {
    /// The bytes of the pack object that hold the record: the offset and the length of its
    /// whole segments, what one ranged GET asks for.
    #[must_use]
    pub fn span(&self) -> (u64, u64) {
        span_in(self.pack.plaintext_size, self.pack.segment_size, self.offset, self.length)
    }
}

/// [`PackedAt::span`] of a record at `offset` of `length` bytes in a pack of `body_len`
/// plaintext bytes and [`PACK_SEGMENT`] segments.
#[must_use]
pub fn segment_span(body_len: u64, offset: u64, length: u64) -> (u64, u64) {
    span_in(body_len, PACK_SEGMENT, offset, length)
}

fn span_in(body_len: u64, segment_size: u32, offset: u64, length: u64) -> (u64, u64) {
    let size = u64::from(segment_size);
    let full = size + SEGMENT_OVERHEAD;
    let count = azl1::segment_count(body_len, segment_size);
    let first = offset / size;
    let last = ((offset + length.max(1) - 1) / size).min(count - 1);
    let last_plain = if last + 1 < count {
        size
    } else {
        body_len - (count - 1) * size
    };
    let start = HEADER_LEN as u64 + first * full;
    let end = HEADER_LEN as u64 + last * full + last_plain + SEGMENT_OVERHEAD;
    (start, end - start)
}

/// The bytes of a pack of `body_len` plaintext bytes in the bucket.
#[must_use]
pub fn pack_object_len(body_len: u64) -> u64 {
    azl1::stored_object_len(body_len, PACK_SEGMENT)
}

/// Where the records of a pack go ([`plan_layout`]).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Layout {
    /// Where each record starts in the pack's plaintext, in the order the lengths came.
    pub offsets: Vec<u64>,
    /// The pack's plaintext bytes: the records and the zeros between them.
    pub body_len: u64,
    /// Whether every record lies inside one data shard of the pack.
    pub aligned: bool,
}

/// Places records of `lengths` bytes in a pack. A pack of [`ALIGN_FROM`] bytes and more puts
/// them into the four data shards, the largest first, each into the first shard with room
/// (first-fit decreasing), growing the pack by zeros until they fit, at most by a sixteenth and
/// never past [`PACK_MAX`]. A smaller pack, and one that does not fit that way, holds them one
/// after the other.
#[must_use]
pub fn plan_layout(lengths: &[u64]) -> Layout {
    let total: u64 = lengths.iter().sum();
    let contiguous = || {
        let mut at = 0;
        let offsets = lengths
            .iter()
            .map(|len| {
                let offset = at;
                at += len;
                offset
            })
            .collect();
        Layout {
            offsets,
            body_len: total,
            aligned: false,
        }
    };
    if pack_object_len(total) < ALIGN_FROM || pack_object_len(total) > PACK_MAX {
        return contiguous();
    }
    let most = largest_body(total + total / 16);
    let mut body = total;
    loop {
        if let Some(offsets) = fit(lengths, body) {
            return Layout {
                offsets,
                body_len: body,
                aligned: true,
            };
        }
        if body >= most {
            return contiguous();
        }
        // Steps of 1/256 keep the padding of a full pack under 1 %.
        body = (body + (body / 256).max(u64::from(PACK_SEGMENT))).min(most);
    }
}

/// The largest plaintext, at most `upto` bytes, whose pack is at most [`PACK_MAX`] bytes.
fn largest_body(upto: u64) -> u64 {
    if pack_object_len(upto) <= PACK_MAX {
        return upto;
    }
    let (mut low, mut high) = (0, upto);
    while low < high {
        let mid = low + (high - low + 1) / 2;
        if pack_object_len(mid) <= PACK_MAX {
            low = mid;
        } else {
            high = mid - 1;
        }
    }
    low
}

/// The records of `lengths` placed by first-fit decreasing into the data shards of a pack of
/// `body` plaintext bytes; `None` when they do not fit.
fn fit(lengths: &[u64], body: u64) -> Option<Vec<u64>> {
    let object_len = pack_object_len(body);
    let shard_len = data_shard_len(object_len);
    let bins: Vec<(u64, u64)> = (0..DATA_SHARDS)
        .filter_map(|shard| bin_of(body, object_len, shard * shard_len, (shard + 1) * shard_len))
        .collect();
    let mut used = vec![0u64; bins.len()];
    let mut order: Vec<usize> = (0..lengths.len()).collect();
    order.sort_by(|a, b| lengths[*b].cmp(&lengths[*a]));
    let mut offsets = vec![0u64; lengths.len()];
    for index in order {
        let len = lengths[index];
        let bin = (0..bins.len()).find(|&b| used[b] + len <= bins[b].1)?;
        offsets[index] = bins[bin].0 + used[bin];
        used[bin] += len;
    }
    Some(offsets)
}

/// The plaintext a data shard covering object bytes `low..high` holds whole: the segments that
/// lie inside it, as (start, capacity) in the plaintext of a pack of `body` bytes; `None` when no
/// segment lies inside it.
fn bin_of(body: u64, object_len: u64, low: u64, high: u64) -> Option<(u64, u64)> {
    let header = HEADER_LEN as u64;
    let size = u64::from(PACK_SEGMENT);
    let full = size + SEGMENT_OVERHEAD;
    let count = azl1::segment_count(body, PACK_SEGMENT);
    let high = high.min(object_len);
    let first = if low <= header {
        0
    } else {
        (low - header).div_ceil(full)
    };
    let sealed_end = |index: u64| {
        let plain = if index + 1 < count {
            size
        } else {
            body - (count - 1) * size
        };
        header + index * full + plain + SEGMENT_OVERHEAD
    };
    if first >= count || sealed_end(first) > high {
        return None;
    }
    // The last segment that ends inside the shard: segments before the last one are all full.
    let mut last = if high >= header + full {
        ((high - header) / full).saturating_sub(1).min(count - 1)
    } else {
        first
    };
    while last > first && sealed_end(last) > high {
        last -= 1;
    }
    if last + 1 < count && sealed_end(last + 1) <= high {
        last += 1;
    }
    let start = first * size;
    let end = ((last + 1) * size).min(body);
    Some((start, end - start))
}

/// One file version on its way into a pack.
pub(crate) struct Member {
    /// The paths that will name it: one for a write; every path that names the version for a
    /// repack (a copy names a version twice).
    targets: Vec<Target>,
    /// The version's own id: its entity tag, new for every write; a repack keeps it (the bytes
    /// are the same).
    version: ObjectId,
    /// The codec byte, then the file, compressed when that paid.
    record: Zeroizing<Vec<u8>>,
    /// The file's bytes before compression.
    size: u64,
    /// BLAKE3 of the file.
    blake3: [u8; 32],
    /// Whether the record is compressed.
    compressed: bool,
}

/// A path a packed member goes to: what it must still hold, and the date its entry keeps.
struct Target {
    /// The drive path that will name the member.
    path: String,
    /// What the path must still hold, else it drops out of the commit (a lost race).
    expect: Expect,
    /// The date its entry keeps.
    modified: Option<u64>,
}

impl Member {
    /// `bytes` for `path` (when `expect` holds), compressed on its own: azlin-compress's member
    /// record - stored, zstd, brotli or a shipped dictionary where that saves 5 %, a whole
    /// container (codec 5) for a small JPEG, PNG or document whose transform pays.
    fn new(path: &str, bytes: &[u8], expect: Expect) -> Result<Member, DriveError> {
        let options = Compression::Auto
            .options(azl1::DEFAULT_SEGMENT_SIZE, codec::extension_of(path))
            .unwrap_or_default();
        let member = azlin_compress::encode_member(bytes, &options)
            .map_err(|e| CryptoError::from(e).for_key(path))?;
        let mut record = Zeroizing::new(Vec::with_capacity(member.data.len() + 1));
        record.push(member.codec);
        record.extend_from_slice(&member.data);
        Ok(Member {
            targets: vec![Target {
                path: path.to_string(),
                expect,
                modified: Some(now_unix()),
            }],
            version: ObjectId::generate().map_err(|e| e.for_key(path))?,
            record,
            size: bytes.len() as u64,
            blake3: *blake3::hash(bytes).as_bytes(),
            compressed: member.codec != 0,
        })
    }

    fn first_path(&self) -> &str {
        self.targets.first().map_or("", |t| t.path.as_str())
    }

    /// The entry a path names this member by, in `sealed` at `offset`, dated `modified`.
    fn entry(&self, sealed: &SealedPack, offset: u64, modified: Option<u64>) -> IndexEntry {
        let length = self.record.len() as u64;
        IndexEntry {
            size: self.size,
            modified,
            object: Some(StoredObject {
                id: self.version,
                stored_size: length,
                blake3: self.blake3,
                wrapped_key: sealed.wrapped_key.clone(),
                compressed: self.compressed,
                packed: Some(PackedAt {
                    pack: sealed.pack.clone(),
                    offset,
                    length,
                }),
            }),
        }
    }
}

/// The plaintext of a record, checked against the index: its size and its BLAKE3.
pub(crate) fn decode_record(
    record: &[u8],
    size: u64,
    expected_hash: &[u8; 32],
    path: &str,
) -> Result<Vec<u8>, DriveError> {
    let corrupt = |reason: &str| DriveError::Corrupt {
        key: path.to_string(),
        reason: reason.to_string(),
    };
    let Some((&codec_byte, data)) = record.split_first() else {
        return Err(corrupt("its record in the pack is empty"));
    };
    let expected = usize::try_from(size).map_err(|_| corrupt("it is larger than memory"))?;
    // The entry's size caps what the record may decode to.
    let plain = azlin_compress::decode_member(codec_byte, data, expected, &Options::default())
        .map_err(|e| CryptoError::from(e).for_key(path))?
        .to_vec();
    if plain.len() != expected || blake3::hash(&plain).as_bytes() != expected_hash {
        return Err(corrupt("the file is not the one the index names"));
    }
    Ok(plain)
}

/// A pack put together: its object, what the index keeps of it, where its members went.
struct SealedPack {
    /// The whole AZL1 object, as it goes into the bucket.
    object: Vec<u8>,
    /// What every member's entry names of the pack.
    pack: PackRef,
    /// The pack's file key, wrapped with the drive key for the pack's id.
    wrapped_key: WrappedKey,
    /// Where each member's record starts in the pack's plaintext, in the members' order.
    offsets: Vec<u64>,
}

/// The pack of `members` under `drive_key` ([`plan_layout`] places them).
fn seal_pack(drive_key: &DriveKey, members: &[Member]) -> Result<SealedPack, CryptoError> {
    let lengths: Vec<u64> = members.iter().map(|m| m.record.len() as u64).collect();
    let layout = plan_layout(&lengths);
    let body_len = usize::try_from(layout.body_len)
        .map_err(|_| CryptoError::Damaged(String::from("a pack larger than memory")))?;
    let mut body = Zeroizing::new(vec![0u8; body_len]);
    for (member, offset) in members.iter().zip(&layout.offsets) {
        let at = *offset as usize;
        body[at..at + member.record.len()].copy_from_slice(&member.record);
    }
    let id = ObjectId::generate()?;
    let options = WriteOptions {
        segment_size: PACK_SEGMENT,
        compression: Compression::Never,
    };
    let (object, summary) = azl1::encrypt(&body, id, drive_key, &options)?;
    if summary.object_len != pack_object_len(layout.body_len)
        || object.len() as u64 != summary.object_len
    {
        return Err(CryptoError::Damaged(String::from(
            "a pack came out at another size than planned",
        )));
    }
    let header = PublicHeader::parse(&object)?;
    Ok(SealedPack {
        pack: PackRef {
            id,
            plaintext_size: layout.body_len,
            object_len: summary.object_len,
            segment_size: PACK_SEGMENT,
            prefix: header.prefix(),
        },
        object,
        wrapped_key: summary.wrapped_key,
        offsets: layout.offsets,
    })
}

/// The mutex's value, also after a thread panicked while holding it.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn io_error(path: &Path, e: &std::io::Error) -> DriveError {
    DriveError::Io(format!("{}: {e}", path.display()))
}

/// A pack in flight, as its journal file says.
#[derive(Serialize, Deserialize)]
struct InFlight {
    /// [`IN_FLIGHT_FORMAT`].
    format: String,
    /// The pack's id (hex).
    pack: String,
    /// When it was sealed (seconds since 1970).
    created: u64,
}

/// The retired packs, as the journal's list says.
#[derive(Serialize, Deserialize)]
struct RetiredList {
    /// [`RETIRED_FORMAT`].
    format: String,
    /// The retired packs, in the order they were retired.
    #[serde(default)]
    packs: Vec<RetiredPack>,
}

#[derive(Serialize, Deserialize)]
struct RetiredPack {
    /// The pack's id (hex).
    pack: String,
    /// When it was retired (seconds since 1970).
    at: u64,
}

/// The packs in flight and the retired packs: files in the journal folder, or memory.
struct Journal {
    /// The journal folder; `None`: everything in [`Journal::memory`].
    dir: Option<PathBuf>,
    /// The state without a folder; with one, the lock that keeps this process's file changes in
    /// order.
    memory: Mutex<JournalMemory>,
}

#[derive(Default)]
struct JournalMemory {
    /// Packs in flight and when each was sealed.
    in_flight: BTreeMap<ObjectId, u64>,
    /// Retired packs and when each was retired.
    retired: Vec<(ObjectId, u64)>,
    /// Packs this device knows of, with their bytes.
    known: BTreeMap<ObjectId, u64>,
}

impl Journal {
    fn new(dir: Option<PathBuf>) -> Journal {
        Journal {
            dir,
            memory: Mutex::new(JournalMemory::default()),
        }
    }

    fn in_flight_file(dir: &Path, pack: &ObjectId) -> PathBuf {
        dir.join(IN_FLIGHT_DIR).join(format!("{}.json", pack.to_hex()))
    }

    /// `pack` is going up, sealed at `now`.
    fn begin(&self, pack: &ObjectId, now: u64) -> Result<(), DriveError> {
        let mut memory = lock(&self.memory);
        let Some(dir) = &self.dir else {
            memory.in_flight.insert(*pack, now);
            return Ok(());
        };
        let folder = dir.join(IN_FLIGHT_DIR);
        fs::create_dir_all(&folder).map_err(|e| io_error(&folder, &e))?;
        let record = InFlight {
            format: IN_FLIGHT_FORMAT.to_string(),
            pack: pack.to_hex(),
            created: now,
        };
        let text = serde_json::to_string(&record).unwrap_or_default();
        let file = Journal::in_flight_file(dir, pack);
        write_atomically(&file, text.as_bytes()).map_err(|e| io_error(&file, &e))
    }

    /// `pack` is no longer in flight: committed, or taken back out of the bucket.
    fn end(&self, pack: &ObjectId) {
        let mut memory = lock(&self.memory);
        match &self.dir {
            None => {
                memory.in_flight.remove(pack);
            }
            Some(dir) => {
                let _ = fs::remove_file(Journal::in_flight_file(dir, pack));
            }
        }
    }

    /// Every pack in flight and when it was sealed. A journal file that does not read counts as
    /// sealed long ago (its pack is cleared up like any stale one).
    fn in_flight(&self) -> Result<Vec<(ObjectId, u64)>, DriveError> {
        let memory = lock(&self.memory);
        let Some(dir) = &self.dir else {
            return Ok(memory.in_flight.iter().map(|(id, at)| (*id, *at)).collect());
        };
        let folder = dir.join(IN_FLIGHT_DIR);
        let entries = match fs::read_dir(&folder) {
            Ok(entries) => entries,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_error(&folder, &e)),
        };
        let mut out = Vec::new();
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(id) = name.strip_suffix(".json").and_then(ObjectId::from_hex) else {
                continue;
            };
            let created = fs::read_to_string(entry.path())
                .ok()
                .and_then(|text| serde_json::from_str::<InFlight>(&text).ok())
                .filter(|record| record.format == IN_FLIGHT_FORMAT)
                .map_or(0, |record| record.created);
            out.push((id, created));
        }
        out.sort();
        Ok(out)
    }

    /// The retired packs and when each was retired.
    fn retired(&self) -> Result<Vec<(ObjectId, u64)>, DriveError> {
        let memory = lock(&self.memory);
        self.retired_locked(&memory)
    }

    fn retired_locked(&self, memory: &JournalMemory) -> Result<Vec<(ObjectId, u64)>, DriveError> {
        let Some(dir) = &self.dir else {
            return Ok(memory.retired.clone());
        };
        let file = dir.join(RETIRED_FILE);
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            Err(e) => return Err(io_error(&file, &e)),
        };
        let list: RetiredList = serde_json::from_str(&text).map_err(|e| {
            DriveError::Io(format!("{}: not a list of retired packs ({e})", file.display()))
        })?;
        if list.format != RETIRED_FORMAT {
            return Err(DriveError::Io(format!(
                "{}: not a list of retired packs (format {:?})",
                file.display(),
                list.format
            )));
        }
        Ok(list
            .packs
            .iter()
            .filter_map(|p| ObjectId::from_hex(&p.pack).map(|id| (id, p.at)))
            .collect())
    }

    fn store_locked(
        &self,
        memory: &mut JournalMemory,
        retired: Vec<(ObjectId, u64)>,
    ) -> Result<(), DriveError> {
        let Some(dir) = &self.dir else {
            memory.retired = retired;
            return Ok(());
        };
        fs::create_dir_all(dir).map_err(|e| io_error(dir, &e))?;
        let list = RetiredList {
            format: RETIRED_FORMAT.to_string(),
            packs: retired
                .iter()
                .map(|(id, at)| RetiredPack {
                    pack: id.to_hex(),
                    at: *at,
                })
                .collect(),
        };
        let text = serde_json::to_string_pretty(&list).unwrap_or_default();
        let file = dir.join(RETIRED_FILE);
        write_atomically(&file, text.as_bytes()).map_err(|e| io_error(&file, &e))
    }

    /// `packs` are retired at `at` (those retired already keep their time).
    fn retire(&self, packs: &[ObjectId], at: u64) -> Result<(), DriveError> {
        let mut memory = lock(&self.memory);
        let mut retired = self.retired_locked(&memory)?;
        for pack in packs {
            if !retired.iter().any(|(id, _)| id == pack) {
                retired.push((*pack, at));
            }
        }
        self.store_locked(&mut memory, retired)
    }

    /// The retired packs are `retired` from now on.
    fn set_retired(&self, retired: Vec<(ObjectId, u64)>) -> Result<(), DriveError> {
        let mut memory = lock(&self.memory);
        self.store_locked(&mut memory, retired)
    }

    /// The packs this device knows of (it wrote them, or saw the index name them), with their
    /// bytes. A pack the index no longer names at all is one whose every file went: only this
    /// list still finds it.
    fn known(&self) -> Result<BTreeMap<ObjectId, u64>, DriveError> {
        let memory = lock(&self.memory);
        self.known_locked(&memory)
    }

    fn known_locked(&self, memory: &JournalMemory) -> Result<BTreeMap<ObjectId, u64>, DriveError> {
        let Some(dir) = &self.dir else {
            return Ok(memory.known.clone());
        };
        let file = dir.join(KNOWN_FILE);
        let text = match fs::read_to_string(&file) {
            Ok(text) => text,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(BTreeMap::new()),
            Err(e) => return Err(io_error(&file, &e)),
        };
        let mut known = BTreeMap::new();
        for line in text.lines() {
            if let Some(rest) = line.strip_prefix('+') {
                let (id, bytes) = rest.split_once(' ').unwrap_or((rest, "0"));
                if let Some(id) = ObjectId::from_hex(id) {
                    known.insert(id, bytes.parse().unwrap_or(0));
                }
            } else if let Some(id) = line.strip_prefix('-').and_then(ObjectId::from_hex) {
                known.remove(&id);
            }
        }
        Ok(known)
    }

    /// This device knows of `packs` (id and bytes): one line each at the end of the list.
    fn note(&self, packs: &[(ObjectId, u64)]) -> Result<(), DriveError> {
        let mut memory = lock(&self.memory);
        let Some(dir) = &self.dir else {
            memory.known.extend(packs.iter().copied());
            return Ok(());
        };
        let lines: String = packs
            .iter()
            .map(|(id, bytes)| format!("+{} {bytes}\n", id.to_hex()))
            .collect();
        Journal::append(dir, &lines)
    }

    /// `packs` left the bucket: the list is written anew without them.
    fn forget(&self, packs: &[ObjectId]) -> Result<(), DriveError> {
        if packs.is_empty() {
            return Ok(());
        }
        let mut memory = lock(&self.memory);
        let mut known = self.known_locked(&memory)?;
        for id in packs {
            known.remove(id);
        }
        let Some(dir) = &self.dir else {
            memory.known = known;
            return Ok(());
        };
        let text: String = known
            .iter()
            .map(|(id, bytes)| format!("+{} {bytes}\n", id.to_hex()))
            .collect();
        let file = dir.join(KNOWN_FILE);
        write_atomically(&file, text.as_bytes()).map_err(|e| io_error(&file, &e))
    }

    fn append(dir: &Path, lines: &str) -> Result<(), DriveError> {
        use std::io::Write as _;
        fs::create_dir_all(dir).map_err(|e| io_error(dir, &e))?;
        let file = dir.join(KNOWN_FILE);
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&file)
            .and_then(|mut f| f.write_all(lines.as_bytes()))
            .map_err(|e| io_error(&file, &e))
    }
}

/// A drive's reads of its packs for the bytes of packed files ([`pack_reads`]); the repack's
/// reads and the hits on a pack kept whole are not counted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct PackReads {
    /// Ranged GETs of a file's record (a read cut short asks again, and counts again).
    pub ranged: u64,
    /// Packs fetched whole, after [`HOT_READS`] reads of one.
    pub whole: u64,
    /// The bytes those requests brought.
    pub bytes: u64,
}

/// The counters behind [`PackReads`], shared by every drive opened under one id.
#[derive(Default)]
struct ReadCounter {
    ranged: AtomicU64,
    whole: AtomicU64,
    bytes: AtomicU64,
}

impl ReadCounter {
    fn ranged(&self, bytes: usize) {
        self.ranged.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    fn whole(&self, bytes: usize) {
        self.whole.fetch_add(1, Ordering::Relaxed);
        self.bytes.fetch_add(bytes as u64, Ordering::Relaxed);
    }

    fn get(&self) -> PackReads {
        PackReads {
            ranged: self.ranged.load(Ordering::Relaxed),
            whole: self.whole.load(Ordering::Relaxed),
            bytes: self.bytes.load(Ordering::Relaxed),
        }
    }
}

/// The read counters of the drives opened by id in this process: an app opens a drive anew
/// for a maintenance round or after a reopen, and its reads still add up.
static COUNTERS: Mutex<BTreeMap<String, Arc<ReadCounter>>> = Mutex::new(BTreeMap::new());

/// What the drives opened as `drive` ([`EncryptedDrive::counting_pack_reads_as`]) read of their
/// packs since this process started; `None` when none was.
#[must_use]
pub fn pack_reads(drive: &str) -> Option<PackReads> {
    lock(&COUNTERS).get(drive).map(|counter| counter.get())
}

/// Packs read recently: how often, and the whole ones kept (ciphertext).
#[derive(Default)]
struct Hot {
    /// Reads per pack since it was last fetched whole ([`HOT_READS`]).
    reads: HashMap<ObjectId, u32>,
    /// The packs fetched whole, oldest first ([`HOT_PACKS`] at most).
    kept: VecDeque<(ObjectId, Arc<Vec<u8>>)>,
}

/// Single writers of small files meeting in one open pack.
#[derive(Default)]
struct Packer {
    /// The open pack's members and the outcomes of sealed ones.
    state: Mutex<Queue>,
    /// Woken when a writer arrives or leaves and when a pack is done.
    changed: Condvar,
}

#[derive(Default)]
struct Queue {
    /// Members waiting for the next pack, by ticket, in the order they came.
    waiting: VecDeque<(u64, Member)>,
    /// Whether a writer is sealing a pack now (one at a time: a pack's writers wait for it).
    sealing: bool,
    /// The outcomes of tickets whose pack is done, until their writers take them.
    done: HashMap<u64, Result<IndexEntry, DriveError>>,
    /// The next writer's ticket.
    next: u64,
    /// Writers inside [`EncryptedDrive::write_packed`]: whether the sealing writer waits for
    /// others.
    writers: usize,
}

/// Unblocks the other writers when a sealing writer stops halfway (a panic): its members fail.
struct SealGuard<'a> {
    /// The packer whose other writers wait.
    packer: &'a Packer,
    /// The tickets of the pack being sealed.
    tickets: Vec<u64>,
    /// Cleared once the outcomes are in: the guard then does nothing.
    armed: bool,
}

impl Drop for SealGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let mut queue = lock(&self.packer.state);
        queue.sealing = false;
        for ticket in self.tickets.drain(..) {
            queue
                .done
                .entry(ticket)
                .or_insert_with(|| Err(DriveError::Io(String::from("the pack writer stopped"))));
        }
        self.packer.changed.notify_all();
    }
}

/// What an encrypted drive keeps for its packs: whether it packs, the journal, the open pack,
/// the hot packs. Every encrypted drive has one: a drive that does not pack still reads packed
/// files and repacks them.
pub(crate) struct PackState {
    /// How the drive packs; `None`: it writes one object per file.
    options: Option<PackOptions>,
    /// Packs in flight, retired packs, known packs.
    journal: Journal,
    /// The open pack of single writes.
    packer: Packer,
    /// Packs read often.
    hot: Mutex<Hot>,
    /// Its reads of packs: its own, or those of every drive opened under its id.
    counted: Arc<ReadCounter>,
}

impl PackState {
    pub(crate) fn new(options: Option<PackOptions>) -> PackState {
        let dir = options.as_ref().and_then(|o| o.journal.clone());
        PackState {
            options,
            journal: Journal::new(dir),
            packer: Packer::default(),
            hot: Mutex::new(Hot::default()),
            counted: Arc::new(ReadCounter::default()),
        }
    }
}

/// What one pack's commit did for each of its members.
struct Committed {
    /// Per member, in order: the entry its paths name now, or why it is not named.
    results: Vec<Result<IndexEntry, DriveError>>,
    /// What the index released (objects nothing names any more).
    released: Vec<ObjectId>,
    /// The packs the index names (two or more when a full drive refused the whole one).
    packs: Vec<PackRef>,
}

impl Committed {
    /// The commit of `self`'s members followed by `other`'s.
    fn then(mut self, other: Committed) -> Committed {
        self.results.extend(other.results);
        self.released.extend(other.released);
        self.packs.extend(other.packs);
        self
    }
}

/// Whether the bucket refused an object for want of room: a smaller one may still fit.
fn drive_full(e: &DriveError) -> bool {
    matches!(
        e,
        DriveError::Service(s)
            if s.status == 507 || matches!(s.azlin_error, Some(crate::AzlinError::QuotaExceeded))
    )
}

/// The files of one pack the head index names: the pack and its live versions.
struct PackUse {
    pack: PackRef,
    /// The live versions: their place, the paths naming them with their entries.
    versions: BTreeMap<ObjectId, (PackedAt, Vec<(String, IndexEntry)>)>,
}

impl PackUse {
    /// The bytes of the live records (a version named twice counts once).
    fn live_bytes(&self) -> u64 {
        self.versions.values().map(|(at, _)| at.length).sum()
    }

    /// The oldest date of a path naming one of its versions.
    fn oldest(&self) -> u64 {
        self.versions
            .values()
            .flat_map(|(_, paths)| paths.iter().map(|(_, e)| e.modified.unwrap_or(0)))
            .min()
            .unwrap_or(0)
    }
}

/// What a repack did ([`EncryptedDrive::repack`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct RepackReport {
    /// Packs whose live files were copied out (and that were retired).
    pub packs_read: usize,
    /// New packs written.
    pub packs_written: usize,
    /// File versions moved (a copy named twice counts once).
    pub files_moved: usize,
    /// Versions whose record did not check out: left where they are.
    pub files_damaged: usize,
    /// Packs none of whose files the index names any more (retired without a read).
    pub packs_emptied: usize,
    /// The bucket bytes of the packs read.
    pub bytes_before: u64,
    /// The bucket bytes of the packs written.
    pub bytes_after: u64,
}

/// What the index names of the drive's packs ([`EncryptedDrive::pack_summary`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackSummary {
    /// Paths whose bytes are in packs (a copy names its version twice).
    pub packed_files: usize,
    /// Packs the index names.
    pub packs: usize,
    /// Paths whose file has an object of its own.
    pub own_objects: usize,
    /// The bucket bytes of those packs: what the quota counts of them.
    pub pack_bytes: u64,
    /// The bytes of their live records; the rest is padding and the garbage of old versions.
    pub live_bytes: u64,
}

/// What a round of upkeep did ([`EncryptedDrive::maintain_packs`]).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct PackUpkeep {
    /// Stale packs in flight cleared up.
    pub recovered: usize,
    /// What the repack did.
    pub repack: RepackReport,
    /// Retired packs deleted.
    pub collected: usize,
}

impl<D: Drive> EncryptedDrive<D> {
    /// Packs small files from now on ([`PackOptions`]); without this the drive writes one object
    /// per file, and still reads packed files.
    #[must_use]
    pub fn with_packing(mut self, options: PackOptions) -> EncryptedDrive<D> {
        let counted = Arc::clone(&self.packs.counted);
        self.packs = PackState::new(Some(options));
        self.packs.counted = counted;
        self
    }

    /// Counts its reads of packs with every other drive opened as `drive` in this process
    /// ([`pack_reads`]).
    #[must_use]
    pub fn counting_pack_reads_as(mut self, drive: &str) -> EncryptedDrive<D> {
        self.packs.counted = Arc::clone(lock(&COUNTERS).entry(drive.to_string()).or_default());
        self
    }

    /// How this drive packs, when it does.
    #[must_use]
    pub fn packing(&self) -> Option<&PackOptions> {
        self.packs.options.as_ref()
    }

    /// The options repacking and the garbage collection follow: the drive's, else the defaults.
    fn upkeep_options(&self) -> PackOptions {
        self.packs.options.clone().unwrap_or_default()
    }

    /// Whether a file of `size` bytes goes into a pack.
    pub(crate) fn packs_size(&self, size: u64) -> bool {
        self.packs
            .options
            .as_ref()
            .is_some_and(|options| size < options.threshold)
    }

    /// The bytes below which files are packed, when the drive packs.
    pub(crate) fn pack_threshold(&self) -> Option<u64> {
        self.packs.options.as_ref().map(|options| options.threshold)
    }

    /// The plaintext of the packed file `path` (its entry `entry`, its object `object`): one
    /// ranged read of its record's segments, or none when its pack is kept.
    pub(crate) fn read_packed(
        &self,
        path: &str,
        entry: &IndexEntry,
        object: &StoredObject,
        packed: &PackedAt,
    ) -> Result<Vec<u8>, DriveError> {
        let pack = &packed.pack;
        let file_key = self
            .drive_key
            .unwrap_file_key(&object.wrapped_key, &pack.id)
            .map_err(|e| e.for_key(path))?;
        let segments = StoredSegments::new(
            &pack.id,
            &file_key,
            &pack.prefix,
            pack.segment_size,
            pack.plaintext_size,
        )
        .map_err(|e| e.for_key(path))?;
        let (offset, len) = segments
            .span(packed.offset, packed.length)
            .map_err(|e| e.for_key(path))?;
        let sealed = self.pack_bytes(path, pack, offset, len)?;
        let record = segments
            .open(packed.offset, packed.length, &sealed)
            .map_err(|e| e.for_key(path))?;
        decode_record(&record, entry.size, &object.blake3, path)
    }

    /// Bytes `offset..offset + len` of `pack`: from a kept pack, from the whole pack when it is
    /// read often, else one ranged GET (resumed when it is cut short).
    fn pack_bytes(
        &self,
        path: &str,
        pack: &PackRef,
        offset: u64,
        len: u64,
    ) -> Result<Vec<u8>, DriveError> {
        let cut = || DriveError::Corrupt {
            key: path.to_string(),
            reason: String::from("its pack is cut short"),
        };
        let slice = |bytes: &[u8]| {
            let end = offset.checked_add(len).filter(|end| *end <= bytes.len() as u64);
            end.map(|end| bytes[offset as usize..end as usize].to_vec())
        };
        let reads = {
            let mut hot = lock(&self.packs.hot);
            if let Some((_, bytes)) = hot.kept.iter().find(|(id, _)| *id == pack.id) {
                return slice(bytes.as_slice()).ok_or_else(cut);
            }
            if hot.reads.len() >= COUNTED_PACKS && !hot.reads.contains_key(&pack.id) {
                hot.reads.clear();
            }
            let count = hot.reads.entry(pack.id).or_insert(0);
            *count += 1;
            *count
        };
        let key = pack.id.bucket_key();
        if reads < HOT_READS {
            let counted = &self.packs.counted;
            return read_resumed(&self.inner, &key, offset, len, counted).map_err(|e| match e {
                DriveError::InvalidRange { .. } => cut(),
                e => read_error(ReadError::Drive(e), path),
            });
        }
        let whole = self
            .inner
            .get(&key)
            .map_err(|e| read_error(ReadError::Drive(e), path))?;
        self.packs.counted.whole(whole.len());
        if whole.len() as u64 != pack.object_len {
            return Err(DriveError::Corrupt {
                key: path.to_string(),
                reason: String::from("its pack is not the one the index names"),
            });
        }
        let out = slice(whole.as_slice()).ok_or_else(cut);
        let mut hot = lock(&self.packs.hot);
        hot.reads.remove(&pack.id);
        hot.kept.push_back((pack.id, Arc::new(whole)));
        while hot.kept.len() > HOT_PACKS {
            hot.kept.pop_front();
        }
        out
    }

    /// Forgets a pack kept in memory (it left the bucket).
    pub(crate) fn forget_pack(&self, pack: &ObjectId) {
        let mut hot = lock(&self.packs.hot);
        hot.kept.retain(|(id, _)| id != pack);
        hot.reads.remove(pack);
    }

    /// How many small files wait for the next pack (a test makes its writers overlap with it).
    #[cfg(test)]
    pub(crate) fn files_waiting_for_a_pack(&self) -> usize {
        lock(&self.packs.packer.state).waiting.len()
    }

    /// Writes the small file `path` into the open pack, sealed with the other writers' files of
    /// the moment: at once when this writer is alone, after at most the linger when writers
    /// overlap. What the listing now shows of it.
    pub(crate) fn write_packed(
        &self,
        path: &str,
        bytes: &[u8],
        expect: Expect,
    ) -> Result<ObjectInfo, DriveError> {
        let member = Member::new(path, bytes, expect)?;
        let linger = self
            .packs
            .options
            .as_ref()
            .map_or(Duration::ZERO, |options| options.linger);
        let packer = &self.packs.packer;
        let mut queue = lock(&packer.state);
        let ticket = queue.next;
        queue.next += 1;
        queue.waiting.push_back((ticket, member));
        queue.writers += 1;
        // A sealing writer that waits for the others hears each one arrive.
        packer.changed.notify_all();
        loop {
            if let Some(result) = queue.done.remove(&ticket) {
                queue.writers -= 1;
                // A sealing writer waiting for everyone counts one fewer now.
                packer.changed.notify_all();
                return result.map(|entry| info_of(path.to_string(), &entry));
            }
            if queue.sealing {
                queue = packer
                    .changed
                    .wait(queue)
                    .unwrap_or_else(PoisonError::into_inner);
                continue;
            }
            queue.sealing = true;
            if queue.writers > 1 && !linger.is_zero() {
                let deadline = Instant::now() + linger;
                loop {
                    let now = Instant::now();
                    let all_here = queue.waiting.len() >= queue.writers;
                    if now >= deadline || all_here || queued_bytes(&queue.waiting) >= PACK_FILL {
                        break;
                    }
                    queue = packer
                        .changed
                        .wait_timeout(queue, deadline - now)
                        .map_or_else(|e| e.into_inner().0, |(guard, _)| guard);
                }
            }
            let batch = take_batch(&mut queue.waiting);
            drop(queue);
            let (tickets, members): (Vec<u64>, Vec<Member>) = batch.into_iter().unzip();
            let mut guard = SealGuard {
                packer,
                tickets: tickets.clone(),
                armed: true,
            };
            let committed = self.commit_pack(&members);
            self.release_except(committed.released, &HashSet::new());
            queue = lock(&packer.state);
            queue.sealing = false;
            for (ticket, result) in tickets.into_iter().zip(committed.results) {
                queue.done.insert(ticket, result);
            }
            guard.armed = false;
            packer.changed.notify_all();
        }
    }

    /// [`crate::Drive::put_many`]: small files into packs of their own, one index change per
    /// pack; folders, big files, and every file of a drive that does not pack, one at a time.
    pub(crate) fn write_many(&self, items: &[BatchPut<'_>]) -> Vec<Result<Option<String>, DriveError>> {
        let mut results: Vec<Option<Result<Option<String>, DriveError>>> =
            (0..items.len()).map(|_| None).collect();
        let mut small: Vec<(usize, Member)> = Vec::new();
        for (i, item) in items.iter().enumerate() {
            let packs = !item.key.ends_with('/') && self.packs_size(item.bytes.len() as u64);
            if !packs {
                results[i] = Some(match item.condition {
                    Some(condition) => self.put_if(item.key, item.bytes, condition),
                    None => self.put(item.key, item.bytes).map(|()| None),
                });
                continue;
            }
            let expect = match item.condition {
                None => Ok(Expect::Any),
                Some(condition) => expect_of(item.key, condition),
            };
            let member = expect
                .and_then(|expect| check_key(item.key).map(|()| expect))
                .and_then(|expect| Member::new(item.key, item.bytes, expect));
            match member {
                Ok(member) => small.push((i, member)),
                Err(e) => results[i] = Some(Err(e)),
            }
        }
        let mut batch: Vec<(usize, Member)> = Vec::new();
        let mut bytes = 0u64;
        let flush = |batch: Vec<(usize, Member)>,
                     results: &mut Vec<Option<Result<Option<String>, DriveError>>>| {
            if batch.is_empty() {
                return;
            }
            let (indices, members): (Vec<usize>, Vec<Member>) = batch.into_iter().unzip();
            let committed = self.commit_pack(&members);
            self.release_except(committed.released, &HashSet::new());
            for ((i, member), result) in indices.into_iter().zip(&members).zip(committed.results) {
                results[i] = Some(
                    result.map(|entry| info_of(member.first_path().to_string(), &entry).etag),
                );
            }
        };
        for (i, member) in small {
            let len = member.record.len() as u64;
            if !batch.is_empty() && bytes + len > PACK_FILL {
                flush(std::mem::take(&mut batch), &mut results);
                bytes = 0;
            }
            bytes += len;
            batch.push((i, member));
        }
        flush(batch, &mut results);
        results
            .into_iter()
            .map(|result| {
                result.unwrap_or_else(|| Err(DriveError::Protocol(String::from("not written"))))
            })
            .collect()
    }

    /// Seals `members` into a new pack, puts it (in the journal while it is in flight) and
    /// names its members in ONE index change; a path whose expectation fails drops out and the
    /// rest is applied again. The index never names the pack before it is in the bucket; a pack
    /// nothing came to name leaves the bucket again; after any other failure the pack stays in
    /// the journal for [`EncryptedDrive::recover_packs`] (the commit may have landed).
    fn commit_pack(&self, members: &[Member]) -> Committed {
        let all = |e: DriveError| Committed {
            results: members.iter().map(|_| Err(e.clone())).collect(),
            released: Vec::new(),
            packs: Vec::new(),
        };
        let first = members.first().map_or("", Member::first_path);
        let sealed = match seal_pack(&self.drive_key, members) {
            Ok(sealed) => sealed,
            Err(e) => return all(e.for_key(first)),
        };
        let id = sealed.pack.id;
        if let Err(e) = self.packs.journal.begin(&id, now_unix()) {
            return all(e);
        }
        if let Err(e) = self.put_object(&id, &sealed.object) {
            // A full drive refuses the pack whole and stores nothing: its members go again as
            // two smaller packs, so the files that still fit go up and only the rest is refused.
            if members.len() > 1 && drive_full(&e) {
                self.packs.journal.end(&id);
                let (first, rest) = members.split_at(members.len() / 2);
                return self.commit_pack(first).then(self.commit_pack(rest));
            }
            // Whether it landed is not known: the journal clears it up.
            return all(e);
        }
        let mut alive: Vec<Vec<bool>> = members
            .iter()
            .map(|m| vec![true; m.targets.len()])
            .collect();
        let mut results: Vec<Option<Result<IndexEntry, DriveError>>> =
            members.iter().map(|_| None).collect();
        let mut released = Vec::new();
        let mut named = false;
        loop {
            let mut changes = Vec::new();
            for (i, member) in members.iter().enumerate() {
                for (t, target) in member.targets.iter().enumerate() {
                    if alive[i][t] {
                        changes.push(IndexChange::Put {
                            path: target.path.clone(),
                            entry: member.entry(&sealed, sealed.offsets[i], target.modified),
                            expect: target.expect.clone(),
                        });
                    }
                }
            }
            if changes.is_empty() {
                break;
            }
            match self.index.apply(changes) {
                Ok(gone) => {
                    released = gone;
                    named = true;
                    break;
                }
                Err(DriveError::Conflict { key }) => {
                    let found = members.iter().enumerate().find_map(|(i, member)| {
                        member
                            .targets
                            .iter()
                            .enumerate()
                            .find(|(t, target)| alive[i][*t] && target.path == key)
                            .map(|(t, _)| (i, t))
                    });
                    let Some((i, t)) = found else {
                        return all(DriveError::Conflict { key });
                    };
                    alive[i][t] = false;
                    if alive[i].iter().all(|a| !a) {
                        results[i] = Some(Err(DriveError::Conflict { key }));
                    }
                }
                Err(e) => {
                    let results = results
                        .into_iter()
                        .map(|r| r.unwrap_or_else(|| Err(e.clone())))
                        .collect();
                    return Committed {
                        results,
                        released: Vec::new(),
                        packs: Vec::new(),
                    };
                }
            }
        }
        if !named {
            // Nothing names the pack: it goes again.
            if self.inner.delete(&id.bucket_key()).is_ok() {
                self.packs.journal.end(&id);
            }
        } else {
            // Best effort: a pack missing from the list is still found while the index names it.
            let _ = self.packs.journal.note(&[(id, sealed.pack.object_len)]);
            self.packs.journal.end(&id);
        }
        let results = members
            .iter()
            .enumerate()
            .map(|(i, member)| {
                results[i].take().unwrap_or_else(|| {
                    let t = alive[i].iter().position(|a| *a).unwrap_or(0);
                    let modified = member.targets.get(t).and_then(|target| target.modified);
                    Ok(member.entry(&sealed, sealed.offsets[i], modified))
                })
            })
            .collect();
        Committed {
            results,
            released,
            packs: named.then_some(sealed.pack).into_iter().collect(),
        }
    }

    /// Deletes what the index released, except `keep` (retired packs wait for their grace).
    fn release_except(&self, released: Vec<ObjectId>, keep: &HashSet<ObjectId>) {
        let gone: Vec<ObjectId> = released.into_iter().filter(|id| !keep.contains(id)).collect();
        for id in &gone {
            self.forget_pack(id);
        }
        self.release(gone);
    }

    /// The packs the head index names, with their live versions.
    fn pack_uses(&self) -> Result<BTreeMap<ObjectId, PackUse>, DriveError> {
        let mut uses: BTreeMap<ObjectId, PackUse> = BTreeMap::new();
        for (path, entry) in self.all_entries("")? {
            let Some(object) = entry.object.as_ref() else {
                continue;
            };
            let Some(packed) = object.packed.clone() else {
                continue;
            };
            let version = object.id;
            let pack_use = uses.entry(packed.pack.id).or_insert_with(|| PackUse {
                pack: packed.pack.clone(),
                versions: BTreeMap::new(),
            });
            pack_use
                .versions
                .entry(version)
                .or_insert_with(|| (packed, Vec::new()))
                .1
                .push((path, entry));
        }
        Ok(uses)
    }

    /// Every bucket object the head index names.
    fn named_objects(&self) -> Result<HashSet<ObjectId>, DriveError> {
        Ok(self
            .all_entries("")?
            .iter()
            .filter_map(|(_, entry)| entry.bucket_object())
            .collect())
    }

    /// Copies the live files of the packs that fell below the repack threshold into new packs,
    /// and merges the small packs when there are two or more; the old packs are retired at
    /// `now` ([`EncryptedDrive::collect_retired`] deletes them after the grace period).
    pub fn repack(&self, now: u64) -> Result<RepackReport, DriveError> {
        let below = self.upkeep_options().repack_below_percent;
        let uses = self.pack_uses()?;
        // The packs this device knows that the index names no more: every file of them went.
        let known = self.packs.journal.known()?;
        let retired: HashSet<ObjectId> = self
            .packs
            .journal
            .retired()?
            .into_iter()
            .map(|(id, _)| id)
            .collect();
        let emptied: Vec<ObjectId> = known
            .keys()
            .filter(|id| !uses.contains_key(*id) && !retired.contains(*id))
            .copied()
            .collect();
        let unknown: Vec<(ObjectId, u64)> = uses
            .values()
            .filter(|u| !known.contains_key(&u.pack.id))
            .map(|u| (u.pack.id, u.pack.object_len))
            .collect();
        if !unknown.is_empty() {
            self.packs.journal.note(&unknown)?;
        }
        let small = uses.values().filter(|u| u.pack.object_len < SMALL_PACK).count();
        let selected: Vec<PackUse> = uses
            .into_values()
            .filter(|u| {
                u.live_bytes() * 100 < u.pack.plaintext_size * below
                    || (small >= 2 && u.pack.object_len < SMALL_PACK)
            })
            .collect();
        let mut report = self.repack_selected(selected, now)?;
        if !emptied.is_empty() {
            self.packs.journal.retire(&emptied, now)?;
            report.packs_emptied = emptied.len();
        }
        Ok(report)
    }

    /// After a key rotation: every pack holding a file dated before `before` is written anew
    /// (a new pack with a new file key, under the drive key this drive holds), and the old packs
    /// leave the bucket at once instead of after the grace period - the wraps in their headers
    /// open with the old drive key.
    pub(crate) fn reencrypt_packs(&self, before: u64, now: u64) -> Result<RepackReport, DriveError> {
        let selected: Vec<PackUse> = self
            .pack_uses()?
            .into_values()
            .filter(|u| u.oldest() < before)
            .collect();
        let old: Vec<ObjectId> = selected.iter().map(|u| u.pack.id).collect();
        let report = self.repack_selected(selected, now)?;
        if !old.is_empty() {
            let named = self.named_objects()?;
            let mut retired = self.packs.journal.retired()?;
            let mut deleted = Vec::new();
            for id in &old {
                if !named.contains(id) && self.inner.delete(&id.bucket_key()).is_ok() {
                    self.forget_pack(id);
                    retired.retain(|(pack, _)| pack != id);
                    deleted.push(*id);
                }
            }
            self.packs.journal.set_retired(retired)?;
            self.packs.journal.forget(&deleted)?;
        }
        Ok(report)
    }

    fn repack_selected(&self, selected: Vec<PackUse>, now: u64) -> Result<RepackReport, DriveError> {
        let mut report = RepackReport::default();
        if selected.is_empty() {
            return Ok(report);
        }
        let old: HashSet<ObjectId> = selected.iter().map(|u| u.pack.id).collect();
        let mut pending: Vec<Member> = Vec::new();
        let mut pending_bytes = 0u64;
        let flush = |pending: Vec<Member>, report: &mut RepackReport| -> Result<(), DriveError> {
            if pending.is_empty() {
                return Ok(());
            }
            let committed = self.commit_pack(&pending);
            self.release_except(committed.released, &old);
            for pack in &committed.packs {
                report.packs_written += 1;
                report.bytes_after += pack.object_len;
            }
            for result in committed.results {
                match result {
                    Ok(_) => report.files_moved += 1,
                    Err(DriveError::Conflict { .. }) => {}
                    Err(e) => return Err(e),
                }
            }
            Ok(())
        };
        for pack_use in &selected {
            let pack = &pack_use.pack;
            let Some((_, (_, paths))) = pack_use.versions.iter().next() else {
                continue;
            };
            let Some((first_path, first_entry)) = paths.first() else {
                continue;
            };
            let Some(wrapped) = first_entry.object.as_ref().map(|o| o.wrapped_key.clone()) else {
                continue;
            };
            let file_key = self
                .drive_key
                .unwrap_file_key(&wrapped, &pack.id)
                .map_err(|e| e.for_key(first_path))?;
            let bytes = self
                .inner
                .get(&pack.id.bucket_key())
                .map_err(|e| read_error(ReadError::Drive(e), first_path))?;
            let body = Zeroizing::new(
                azl1::decrypt(&bytes, &pack.id, &file_key).map_err(|e| e.for_key(first_path))?,
            );
            report.packs_read += 1;
            report.bytes_before += pack.object_len;
            for (version, (at, paths)) in &pack_use.versions {
                let Some((path, entry)) = paths.first() else {
                    continue;
                };
                let Some(object) = entry.object.as_ref() else {
                    continue;
                };
                let end = at.offset.checked_add(at.length).filter(|end| *end <= body.len() as u64);
                let Some(end) = end else {
                    report.files_damaged += 1;
                    continue;
                };
                let record = &body[at.offset as usize..end as usize];
                if decode_record(record, entry.size, &object.blake3, path).is_err() {
                    report.files_damaged += 1;
                    continue;
                }
                let targets = paths
                    .iter()
                    .map(|(path, entry)| Target {
                        path: path.clone(),
                        expect: Expect::Object(*version),
                        modified: entry.modified,
                    })
                    .collect();
                let member = Member {
                    targets,
                    version: *version,
                    record: Zeroizing::new(record.to_vec()),
                    size: entry.size,
                    blake3: object.blake3,
                    compressed: object.compressed,
                };
                let len = member.record.len() as u64;
                if !pending.is_empty() && pending_bytes + len > PACK_FILL {
                    flush(std::mem::take(&mut pending), &mut report)?;
                    pending_bytes = 0;
                }
                pending_bytes += len;
                pending.push(member);
            }
        }
        flush(pending, &mut report)?;
        let retired: Vec<ObjectId> = old.into_iter().collect();
        self.packs.journal.retire(&retired, now)?;
        Ok(report)
    }

    /// How the index's files are stored: in packs, or in objects of their own. Walks the whole
    /// index: call it from a worker thread.
    pub fn pack_summary(&self) -> Result<PackSummary, DriveError> {
        let mut summary = PackSummary::default();
        let mut packs: HashMap<ObjectId, u64> = HashMap::new();
        let mut versions: HashMap<ObjectId, u64> = HashMap::new();
        for (_, entry) in self.all_entries("")? {
            let Some(object) = entry.object.as_ref() else {
                continue;
            };
            match &object.packed {
                Some(packed) => {
                    summary.packed_files += 1;
                    packs.insert(packed.pack.id, packed.pack.object_len);
                    versions.insert(object.id, packed.length);
                }
                None => summary.own_objects += 1,
            }
        }
        summary.packs = packs.len();
        summary.pack_bytes = packs.values().sum();
        summary.live_bytes = versions.values().sum();
        Ok(summary)
    }

    /// The retired packs and when each was retired.
    #[must_use]
    pub fn retired_packs(&self) -> Vec<(ObjectId, u64)> {
        self.packs.journal.retired().unwrap_or_default()
    }

    /// Deletes the retired packs whose grace period ended by `now`; a retired pack the index
    /// names again (a restore brought an old version back) is no longer retired. How many left
    /// the bucket.
    pub fn collect_retired(&self, now: u64) -> Result<usize, DriveError> {
        let grace = self.upkeep_options().retired_grace_secs;
        let retired = self.packs.journal.retired()?;
        if !retired.iter().any(|(_, at)| at.saturating_add(grace) <= now) {
            return Ok(0);
        }
        let named = self.named_objects()?;
        let mut keep = Vec::new();
        let mut deleted = Vec::new();
        for (id, at) in retired {
            if named.contains(&id) {
                continue;
            }
            if at.saturating_add(grace) > now {
                keep.push((id, at));
                continue;
            }
            match self.inner.delete(&id.bucket_key()) {
                Ok(()) => {
                    self.forget_pack(&id);
                    deleted.push(id);
                }
                Err(_) => keep.push((id, at)),
            }
        }
        self.packs.journal.set_retired(keep)?;
        self.packs.journal.forget(&deleted)?;
        Ok(deleted.len())
    }

    /// Clears up the packs whose writer stopped between the upload and the journal's end (a
    /// crash, a commit whose answer was lost): one the index names stays, one it does not name
    /// leaves the bucket. Packs in flight for less than [`JOURNAL_STALE_SECS`] at `now` are left
    /// alone, their writer may still commit them. How many were cleared up.
    pub fn recover_packs(&self, now: u64) -> Result<usize, DriveError> {
        let stale: Vec<ObjectId> = self
            .packs
            .journal
            .in_flight()?
            .into_iter()
            .filter(|(_, created)| created.saturating_add(JOURNAL_STALE_SECS) < now)
            .map(|(id, _)| id)
            .collect();
        if stale.is_empty() {
            return Ok(0);
        }
        let named = self.named_objects()?;
        for id in &stale {
            if !named.contains(id) {
                self.inner.delete(&id.bucket_key())?;
                self.forget_pack(id);
            }
            self.packs.journal.end(id);
        }
        Ok(stale.len())
    }

    /// One round of upkeep for an idle computer: the stale packs in flight, a repack, the retired
    /// packs whose grace ended.
    pub fn maintain_packs(&self, now: u64) -> Result<PackUpkeep, DriveError> {
        Ok(PackUpkeep {
            recovered: self.recover_packs(now)?,
            repack: self.repack(now)?,
            collected: self.collect_retired(now)?,
        })
    }
}

/// The record bytes of the members waiting.
fn queued_bytes(waiting: &VecDeque<(u64, Member)>) -> u64 {
    waiting.iter().map(|(_, m)| m.record.len() as u64).sum()
}

/// The members of the next pack, in order: at least one, then as many as fill it.
fn take_batch(waiting: &mut VecDeque<(u64, Member)>) -> Vec<(u64, Member)> {
    let mut batch = Vec::new();
    let mut bytes = 0u64;
    while let Some((_, member)) = waiting.front() {
        let len = member.record.len() as u64;
        if !batch.is_empty() && bytes + len > PACK_FILL {
            break;
        }
        bytes += len;
        if let Some(next) = waiting.pop_front() {
            batch.push(next);
        }
    }
    batch
}

/// Bytes `offset..offset + len` of `key`, in one ranged GET; a body cut short, or a request
/// without an answer, asks for the rest, at most [`RESUME_ATTEMPTS`] times more. Each GET counts
/// in `counted`.
fn read_resumed<D: Drive + ?Sized>(
    drive: &D,
    key: &str,
    offset: u64,
    len: u64,
    counted: &ReadCounter,
) -> Result<Vec<u8>, DriveError> {
    let mut out = Vec::with_capacity(usize::try_from(len).unwrap_or(0));
    let mut retries = 0;
    while (out.len() as u64) < len {
        let from = offset + out.len() as u64;
        let range = ByteRange::new(from, Some(offset + len - 1));
        let answer = drive.get_range(key, range);
        counted.ranged(answer.as_ref().map_or(0, Vec::len));
        match answer {
            Ok(bytes) if !bytes.is_empty() => {
                let want = usize::try_from(len - out.len() as u64).unwrap_or(usize::MAX);
                out.extend_from_slice(&bytes[..bytes.len().min(want)]);
                if (out.len() as u64) < len {
                    retries += 1;
                }
            }
            Ok(_) => {
                return Err(DriveError::InvalidRange {
                    key: key.to_string(),
                })
            }
            Err(DriveError::Transport(_)) if retries < RESUME_ATTEMPTS => retries += 1,
            Err(e) => return Err(e),
        }
        if retries > RESUME_ATTEMPTS {
            return Err(DriveError::InvalidRange {
                key: key.to_string(),
            });
        }
    }
    Ok(out)
}
