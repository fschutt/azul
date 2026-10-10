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
//! The pointer is inside the repository, so it is sealed with everything else
//! in the bucket; it holds no secret in the clear (the file key is wrapped).

use std::fmt::Write as _;

use super::{from_hex, hex};
use crate::{
    crypto::{ObjectId as DataId, WrappedKey},
    encrypted::{IndexEntry, StoredObject},
};

/// The first line of every pointer.
pub const MAGIC: &str = "azlin-pointer 1";

/// The pointer blob of `entry`.
#[must_use]
pub fn encode(entry: &IndexEntry) -> Vec<u8> {
    let mut out = String::with_capacity(320);
    out.push_str(MAGIC);
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
    }
    out.into_bytes()
}

/// The entry of a pointer blob [`encode`] wrote. Lines it does not know are
/// skipped (a later version may add some).
pub fn decode(bytes: &[u8]) -> Result<IndexEntry, String> {
    let text = std::str::from_utf8(bytes).map_err(|_| "a pointer that is not UTF-8")?;
    let mut lines = text.lines();
    if lines.next() != Some(MAGIC) {
        return Err("not a pointer of this format".to_string());
    }
    let mut size = None;
    let mut modified = None;
    let mut id = None;
    let mut stored = None;
    let mut blake3 = None;
    let mut key = None;
    let mut compressed = false;
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
            _ => {}
        }
    }
    let object = match id {
        None => None,
        Some(id) => Some(StoredObject {
            id,
            stored_size: stored.ok_or("a pointer without the object's size")?,
            blake3: blake3.ok_or("a pointer without the hash")?,
            wrapped_key: key.ok_or("a pointer without the wrapped key")?,
            compressed,
        }),
    };
    Ok(IndexEntry {
        size: size.ok_or("a pointer without the size")?,
        modified,
        object,
    })
}
