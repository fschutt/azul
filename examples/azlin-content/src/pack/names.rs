//! The names of a pack: the hash that names it between devices, the 16-byte id that is the last part
//! of its storage key, and the storage key itself (planning/architecture.md section 5.7 "plus each
//! pack's ciphertext BLAKE3 (the same hash the store keeps as the stripe's, and the pack's name between
//! devices, 5.13)"; section 5.4 "`s/<space>/p/<2 hex>/<32 hex>` packs (AZP2)"; build unit S14, lane
//! W1-S14c, brief P-3).
//!
//! The name is the BLAKE3 hash of the pack's whole ciphertext, the header and the trailer included. The
//! id is the first 16 bytes of the name, so the storage key is a function of the pack's bytes: a pack
//! sent twice lands on one key (planning/architecture.md section 2.5 "Idempotent by construction"), and
//! a different byte anywhere is a different pack. DRIFT X-DELTA-W1S14C-5: the plan does not say how the
//! 32 hex digits of the storage key arise; whether a client may choose a random key instead is open.

use azlin_keys::SpaceId;

use crate::ids::{PackId, PackName};

/// The name of a pack: the BLAKE3 hash of its ciphertext, which is what the chunk index records and
/// what iroh-blobs checks the bytes against while they stream (planning/architecture.md section 5.13).
///
/// Caller: `PackBuilder::finish` (INT-S14-1), S14e1 (`rebuild_from_trailers` recomputes the name of a
/// pack it read), INT-S14-3 (a downloaded pack is checked against the name the index records).
pub fn pack_name(bytes: &[u8]) -> PackName {
    PackName::from_bytes(*blake3::hash(bytes).as_bytes())
}

/// The id of a pack: the first 16 bytes of its name (DRIFT X-DELTA-W1S14C-5, brief P-3).
///
/// Caller: `PackBuilder::finish`, S14d (`ChunkRef::pack`), S14e1 (`PackInfo`).
pub fn pack_id(name: &PackName) -> PackId {
    let mut id = [0u8; PackId::LEN];
    for (byte, source) in id.iter_mut().zip(name.as_bytes()) {
        *byte = *source;
    }
    PackId::from_bytes(id)
}

/// The key a pack is stored under: `s/<space as 32 hex digits>/p/<first byte of the id as 2 hex
/// digits>/<id as 32 hex digits>` (planning/architecture.md section 5.4), all lowercase.
///
/// Caller: INT-S14-1 and INT-S14-3 (azul-storage's upload and download of a pack), INT-S14-4 (the
/// node's layout of a space's packs).
pub fn storage_key(space: &SpaceId, id: &PackId) -> String {
    let &[first, ..] = id.as_bytes();
    format!("s/{space}/p/{first:02x}/{id}")
}
