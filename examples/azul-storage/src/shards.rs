//! Where an object's bytes lie on an Azlin server's disks: the geometry of its 4 + 2 code.
//!
//! A server cuts every object into chunks of [`CHUNK_SIZE`] bytes and codes each chunk into
//! [`DATA_SHARDS`] data shards and 2 parity shards on six disks. The code is systematic: data
//! shard `j` of a chunk is the plain slice `j * s .. (j + 1) * s` of it, `s` being a quarter of
//! the chunk rounded up to 64 bytes ([`data_shard_len`]). So a ranged read inside one data shard
//! costs one seek on one disk, and the parity shards are only read while a disk is down.
//!
//! Packing places small files so that each lies in one data shard (`packing`); a client that
//! codes its own shards (mode (a)) reads a range as the [`ShardRead`]s [`shard_reads`] lists:
//! one request per data shard it touches, at that shard's own offset.

/// The server's chunk: an object is cut into chunks of this many bytes, each coded on its own.
pub const CHUNK_SIZE: u64 = 4 << 20;
/// Data shards per chunk.
pub const DATA_SHARDS: u64 = 4;
/// Parity shards per chunk: any [`DATA_SHARDS`] of the six rebuild the chunk.
pub const PARITY_SHARDS: u64 = 2;

/// The bytes of each data shard of a chunk of `chunk_len` bytes, as the server codes it: a
/// quarter, rounded up to whole 64-byte blocks.
#[must_use]
pub fn data_shard_len(chunk_len: u64) -> u64 {
    chunk_len.div_ceil(DATA_SHARDS).max(1).div_ceil(64) * 64
}

/// One read of a data shard: part of a range of an object.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardRead {
    /// The chunk (the object's bytes from `chunk * CHUNK_SIZE` on).
    pub chunk: u64,
    /// The data shard of the chunk, 0 to 3: its position in the code.
    pub shard: u64,
    /// Where the read starts in the shard.
    pub offset: u64,
    /// The bytes it reads.
    pub len: u64,
}

/// The data shard reads that make bytes `start..start + len` of an object of `object_len`
/// bytes, in order (bytes past the object's end are not read).
#[must_use]
pub fn shard_reads(object_len: u64, start: u64, len: u64) -> Vec<ShardRead> {
    let mut out = Vec::new();
    let end = start.saturating_add(len).min(object_len);
    let mut at = start;
    while at < end {
        let chunk = at / CHUNK_SIZE;
        let chunk_start = chunk * CHUNK_SIZE;
        let chunk_len = (object_len - chunk_start).min(CHUNK_SIZE);
        let shard_len = data_shard_len(chunk_len);
        let shard = (at - chunk_start) / shard_len;
        let shard_start = chunk_start + shard * shard_len;
        let shard_end = (shard_start + shard_len).min(chunk_start + chunk_len);
        let read_end = shard_end.min(end);
        out.push(ShardRead {
            chunk,
            shard,
            offset: at - shard_start,
            len: read_end - at,
        });
        at = read_end;
    }
    out
}

/// The data shards, as (chunk, shard) pairs in order, that hold bytes `start..start + len` of an
/// object of `object_len` bytes: what a read of them touches on the server's disks.
#[must_use]
pub fn data_shards_of(object_len: u64, start: u64, len: u64) -> Vec<(u64, u64)> {
    shard_reads(object_len, start, len)
        .into_iter()
        .map(|read| (read.chunk, read.shard))
        .collect()
}
