//! Reading a pack: the trailer by a ranged read, one chunk by a ranged read, and the whole pack when
//! a lost chunk index is rebuilt (planning/architecture.md section 5.7 "The server sees an opaque
//! 4 MiB object, and a lost chunk index can be rebuilt from the trailers"; build unit S14, lane
//! W1-S14c).
//!
//! A client that wants one chunk needs nothing but the pointer's `(pack, offset, stored length)`:
//! [`chunk_range`] checks the entry against the header and gives the bytes to read. A client that wants
//! the trailer reads the header first ([`crate::pack::PackHeader::parse`] on the first 24 bytes),
//! then the bytes of [`crate::pack::PackHeader::trailer_range`], and opens them with [`read_trailer`].
//! Both read only part of the pack, so neither can check the body: only [`rebuild_entries`], which
//! holds the whole pack, checks the body against the hash the trailer sealed and the padding against
//! its keystream (DRIFT X-DELTA-W1S14C-7).

use core::ops::Range;

use azlin_keys::SpaceId;

use super::header::PackHeader;
use super::padding::keystream;
use super::trailer::{open, NONCE_LEN};
use super::{PackError, SealKey};
use crate::ids::PackEntry;
use crate::units::{PackLen, PackOffset};

/// Opens the trailer of a pack from its header and the bytes of its trailer: exactly the bytes of
/// [`PackHeader::trailer_range`], read from the store by a ranged read. The result is the list of
/// `(chunk_id, offset, length)` lines, authenticated under `K_seal`: a wrong key, space or epoch, a
/// trailer of another pack, and any flipped bit are [`PackError::Auth`]; a header that is not
/// consistent with the length of `trailer` is [`PackError::SizesInconsistent`] (or its siblings); a
/// trailer that authenticates but is not the format is [`PackError::BadTrailer`]. It does NOT check the
/// body, which the caller has not read: [`rebuild_entries`] does. DRIFT X-DELTA-W1S14C-7: the brief's
/// `trailer_and_header` is exactly these trailer bytes (the parsed `header` is the header).
///
/// Caller: INT-S14-2 and INT-S14-3 (azlin-client reads the trailer of a pack it fetched by ranges),
/// S14e1 (the index rebuild when only trailers were fetched).
pub fn read_trailer(
    header: &PackHeader,
    trailer: &[u8],
    space: &SpaceId,
    key: &SealKey,
) -> Result<Vec<PackEntry>, PackError> {
    Ok(open(key, space, header, trailer)?.entries)
}

/// The bytes of one chunk blob inside its pack: the range of `entry`, checked to lie wholly inside the
/// body that `header` describes, so that a ranged read of exactly these bytes is the blob and
/// nothing else is needed (the pointer holds `(pack, offset, stored length)`). An entry that starts
/// inside the header, ends past the body, or overflows is [`PackError::EntryOutsideBody`].
///
/// Caller: INT-S14-3 (azlin-client's `object_get` of one chunk), S14e2 (the repack reads the live
/// blobs of a pack).
pub fn chunk_range(entry: &PackEntry, header: &PackHeader) -> Result<Range<PackOffset>, PackError> {
    let body = header.body_range()?;
    let end = entry.end().ok_or(PackError::EntryOutsideBody)?;
    if entry.offset < body.start || end > body.end {
        return Err(PackError::EntryOutsideBody);
    }
    Ok(entry.offset..end)
}

/// The lines of the trailer of a whole pack, with everything that can be checked checked: the header
/// ([`PackHeader::parse`] against the length of `pack_bytes`), the trailer ([`read_trailer`]), the body
/// against the BLAKE3 hash the trailer sealed, and the padding against its keystream. This is "a lost
/// chunk index can be rebuilt from the trailers" (planning/architecture.md section 5.7): the index is
/// the lines of every pack. Any failure is a typed error and there is never a partial list: a truncated
/// pack is [`PackError::SizesInconsistent`] or [`PackError::Auth`], a damaged trailer
/// [`PackError::Auth`] or [`PackError::BadTrailer`], a flipped bit in the header, body or padding
/// [`PackError::Auth`] (or the header error of the field it hit).
///
/// Caller: S14e1 (`rebuild_from_trailers` over the packs of a space), INT-S14-3 (a downloaded pack is
/// opened before its entries are indexed), S14e2 (verifies a pack before it repacks it).
pub fn rebuild_entries(
    pack_bytes: &[u8],
    space: &SpaceId,
    key: &SealKey,
) -> Result<Vec<PackEntry>, PackError> {
    let total_len = PackLen::from_len(pack_bytes.len()).ok_or(PackError::TooLong)?;
    let header = PackHeader::parse(pack_bytes, total_len)?;
    let trailer_at = header.trailer_range(total_len)?;
    let (before, trailer) = pack_bytes
        .split_at_checked(trailer_at.start.as_usize())
        .ok_or(PackError::SizesInconsistent)?;
    let opened = open(key, space, &header, trailer)?;

    // The body and the padding, in the bytes between the header and the trailer.
    let (_header_bytes, rest) = before
        .split_at_checked(PackHeader::LEN)
        .ok_or(PackError::SizesInconsistent)?;
    let (body, padding) = rest
        .split_at_checked(header.body_len.as_usize())
        .ok_or(PackError::SizesInconsistent)?;
    if *blake3::hash(body).as_bytes() != opened.body_hash {
        return Err(PackError::Auth);
    }

    // The padding is the keystream of the key and of the trailer's nonce. The buffer is as long as the
    // padding the pack really has (it is a part of `pack_bytes`), never as long as a claim.
    let (nonce, _) = trailer
        .split_first_chunk::<NONCE_LEN>()
        .ok_or(PackError::SizesInconsistent)?;
    let mut expected = vec![0u8; padding.len()];
    keystream(key, nonce, &mut expected)?;
    if expected.as_slice() != padding {
        return Err(PackError::Auth);
    }
    Ok(opened.entries)
}
