//! A drive file's pointer blob (feature `encryption`): what the encrypted
//! drive's index keeps for one path ([`IndexEntry`]), as a small text file at
//! the file's path in the tree. Text, so `git diff` shows what changed; the
//! first line names the format.
//!
//! ```text
//! azlin-pointer 1
//! size 4711                     the plaintext's bytes
//! modified 1760000000           seconds since 1970 (absent: unknown)
//! object 0123...cdef            the AZL1 object (32 hex digits; absent: no object)
//! stored 5016                   the object's bytes in the bucket
//! blake3 89ab...                the BLAKE3 of the plaintext (64 hex digits)
//! key 0011...                   the file key wrapped with the drive key (hex)
//! compressed 0                  1 when a segment is compressed
//! ```
//!
//! A small file packed with others into one object ([`crate::packing`]) has a pointer of the
//! second format, so a build that does not know packs refuses it instead of reading the pack as
//! the file:
//!
//! ```text
//! azlin-pointer 2
//! size 4711
//! modified 1760000000
//! object 0123...cdef            this version's own id (its entity tag), no object of its own
//! stored 1203                   its record's bytes in the pack
//! blake3 89ab...
//! key 0011...                   the PACK's file key wrapped with the drive key
//! compressed 1                  1 when its record is compressed
//! pack 4567...89ab 3911002 4194304 4096 00ff...   the pack's id, plaintext bytes, bytes in the
//!                                                 bucket, segment size and nonce prefix
//! at 81920 1203                 where its record starts in the pack's plaintext, its bytes
//! ```
//!
//! The pointer is inside the repository, so it is sealed with everything else
//! in the bucket; it holds no secret in the clear (the file key is wrapped).

use std::fmt::Write as _;

use super::{from_hex, hex};
use crate::{
    crypto::{azl1::PREFIX_LEN, ObjectId as DataId, WrappedKey},
    encrypted::{IndexEntry, StoredObject},
    packing::{PackRef, PackedAt},
};

/// The first line of every pointer of a file with an object of its own.
pub const MAGIC: &str = "azlin-pointer 1";
/// The first line of every pointer of a packed file.
pub const MAGIC_PACKED: &str = "azlin-pointer 2";

/// The format of a pointer newer than the ones this version reads (`azlin-pointer N`, N above
/// 2): a newer version of the app wrote the drive index. `None` for a pointer this version
/// reads, and for what is no pointer at all.
#[must_use]
pub fn newer_format(bytes: &[u8]) -> Option<u32> {
    let first = std::str::from_utf8(bytes).ok()?.lines().next()?;
    let format: u32 = first.strip_prefix("azlin-pointer ")?.trim().parse().ok()?;
    (format > 2).then_some(format)
}

/// What a pointer of the newer `format` is, in words: never damage - an update opens it.
#[must_use]
pub fn newer_reason(format: u32) -> String {
    format!(
        "it was written by a newer version of the app (pointer format {format}): update the \
         app to open it"
    )
}

/// The pointer blob of `entry`.
#[must_use]
pub fn encode(entry: &IndexEntry) -> Vec<u8> {
    let mut out = String::with_capacity(320);
    let packed = entry.object.as_ref().and_then(|o| o.packed.as_ref());
    out.push_str(if packed.is_some() { MAGIC_PACKED } else { MAGIC });
    out.push('\n');
    let _ = writeln!(out, "size {}", entry.size);
    if let Some(modified) = entry.modified {
        let _ = writeln!(out, "modified {modified}");
    }
    if let Some(object) = &entry.object {
        let _ = writeln!(out, "object {}", object.id.to_hex());
        let _ = writeln!(out, "stored {}", object.stored_size);
        let _ = writeln!(out, "blake3 {}", hex(&object.blake3));
        let _ = writeln!(out, "key {}", object.wrapped_key.to_hex());
        let _ = writeln!(out, "compressed {}", u8::from(object.compressed));
        if let Some(packed) = &object.packed {
            let pack = &packed.pack;
            let _ = writeln!(
                out,
                "pack {} {} {} {} {}",
                pack.id.to_hex(),
                pack.plaintext_size,
                pack.object_len,
                pack.segment_size,
                hex(&pack.prefix)
            );
            let _ = writeln!(out, "at {} {}", packed.offset, packed.length);
        }
    }
    out.into_bytes()
}

/// The pack of a pointer's `pack` line.
fn pack_of(value: &str) -> Result<PackRef, String> {
    let fields: Vec<&str> = value.split(' ').collect();
    let [id, plaintext, object_len, segment, prefix] = fields.as_slice() else {
        return Err("a pointer's pack line is damaged".to_string());
    };
    fn number(text: &str) -> Result<u64, String> {
        text.parse::<u64>()
            .map_err(|_| "a pointer's pack line has a damaged number".to_string())
    }
    let prefix: [u8; PREFIX_LEN] = from_hex(prefix)
        .and_then(|bytes| bytes.try_into().ok())
        .ok_or("a pointer's pack prefix is damaged")?;
    Ok(PackRef {
        id: DataId::from_hex(id).ok_or("a pointer's pack id is damaged")?,
        plaintext_size: number(plaintext)?,
        object_len: number(object_len)?,
        segment_size: u32::try_from(number(segment)?)
            .map_err(|_| "a pointer's pack segment size is damaged")?,
        prefix,
    })
}

/// The entry of a pointer blob [`encode`] wrote. Lines it does not know are
/// skipped (a later version may add some).
pub fn decode(bytes: &[u8]) -> Result<IndexEntry, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "a pointer that is not UTF-8")?;
    let mut lines = text.lines();
    let packed_format = match lines.next() {
        Some(MAGIC) => false,
        Some(MAGIC_PACKED) => true,
        _ => {
            return Err(match newer_format(bytes) {
                Some(format) => newer_reason(format),
                None => "not a pointer of this format".to_string(),
            })
        }
    };
    let mut size = None;
    let mut modified = None;
    let mut id = None;
    let mut stored = None;
    let mut blake3 = None;
    let mut key = None;
    let mut compressed = false;
    let mut pack = None;
    let mut at = None;
    for line in lines {
        let (field, value) = line.split_once(' ').unwrap_or((line, ""));
        let number = || value.parse::<u64>().map_err(|_| format!("a pointer's {field} is no number"));
        match field {
            "size" => size = Some(number()?),
            "modified" => modified = Some(number()?),
            "object" => id = Some(DataId::from_hex(value).ok_or("a pointer's object id is damaged")?),
            "stored" => stored = Some(number()?),
            "blake3" => {
                let bytes = from_hex(value).ok_or("a pointer's hash is damaged")?;
                let hash: [u8; 32] = bytes
                    .try_into()
                    .map_err(|_| "a pointer's hash is not 32 bytes")?;
                blake3 = Some(hash);
            }
            "key" => {
                key = Some(WrappedKey::from_hex(value).ok_or("a pointer's wrapped key is damaged")?);
            }
            "compressed" => compressed = value == "1",
            "pack" if packed_format => pack = Some(pack_of(value)?),
            "at" if packed_format => {
                let (offset, length) = value
                    .split_once(' ')
                    .and_then(|(o, l)| Some((o.parse::<u64>().ok()?, l.parse::<u64>().ok()?)))
                    .ok_or("a pointer's place in its pack is damaged")?;
                at = Some((offset, length));
            }
            _ => {}
        }
    }
    let packed = match (packed_format, pack, at) {
        (false, _, _) => None,
        (true, Some(pack), Some((offset, length))) => Some(PackedAt {
            pack,
            offset,
            length,
        }),
        (true, _, _) => return Err("a packed file's pointer without its pack".to_string()),
    };
    let object = match id {
        None => None,
        Some(id) => Some(StoredObject {
            id,
            stored_size: stored.ok_or("a pointer without the object's size")?,
            blake3: blake3.ok_or("a pointer without the hash")?,
            wrapped_key: key.ok_or("a pointer without the wrapped key")?,
            compressed,
            packed,
        }),
    };
    Ok(IndexEntry {
        size: size.ok_or("a pointer without the size")?,
        modified,
        object,
    })
}
