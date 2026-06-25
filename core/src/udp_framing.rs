//! UDP chunked-message framing (SUPER_PLAN_2 P8).
//!
//! Splits a large payload (a video keyframe) into sequenced datagrams and
//! reassembles them on the far side, tolerating reorder + loss: a message whose
//! chunks never all arrive is dropped (bounded memory), never retransmitted -
//! the fault-tolerant model realtime A/V wants. This is pure (no socket), so
//! the dll's `Udp` handle builds on it and the logic is unit-testable here.
//!
//! Wire format per datagram: an 8-byte little-endian header
//! (`msg_id: u32`, `chunk_idx: u16`, `chunk_count: u16`) followed by the chunk
//! payload.

use alloc::collections::BTreeMap;
use alloc::vec::Vec;

/// Datagram header length: `msg_id` (u32) + `chunk_idx` (u16) + `chunk_count` (u16).
pub const CHUNK_HEADER_LEN: usize = 8;
/// Conservative per-datagram payload (leaves room under a ~1400-byte path MTU).
pub const DEFAULT_CHUNK_PAYLOAD: usize = 1200;
/// Cap on in-flight partial messages; the oldest is evicted past this so a lost
/// chunk can never leak memory.
const MAX_PARTIAL_MESSAGES: usize = 256;

/// Split `data` into chunk datagrams for `msg_id`.
///
/// Each datagram is
/// `CHUNK_HEADER_LEN + <= max_payload` bytes. An empty payload still produces
/// one (header-only) chunk, so a zero-length message round-trips.
#[must_use] pub fn chunk_message(msg_id: u32, data: &[u8], max_payload: usize) -> Vec<Vec<u8>> {
    let max_payload = max_payload.max(1);
    let count = if data.is_empty() {
        1
    } else {
        data.len().div_ceil(max_payload)
    };
    let count_u16 = u16::try_from(count).unwrap_or(u16::MAX);
    let mut out = Vec::with_capacity(count_u16 as usize);
    for idx in 0..count_u16 {
        let start = idx as usize * max_payload;
        let end = (start + max_payload).min(data.len());
        let mut p = Vec::with_capacity(CHUNK_HEADER_LEN + end.saturating_sub(start));
        p.extend_from_slice(&msg_id.to_le_bytes());
        p.extend_from_slice(&idx.to_le_bytes());
        p.extend_from_slice(&count_u16.to_le_bytes());
        if start < end {
            p.extend_from_slice(&data[start..end]);
        }
        out.push(p);
    }
    out
}

#[derive(Debug)]
struct PartialMessage {
    count: u16,
    chunks: BTreeMap<u16, Vec<u8>>,
}

/// Reassembles chunk datagrams into complete messages, tolerating out-of-order
/// delivery and dropping incomplete messages once too many pile up.
#[derive(Debug, Default)]
pub struct UdpReassembler {
    partial: BTreeMap<u32, PartialMessage>,
}

impl UdpReassembler {
    #[must_use] pub fn new() -> Self {
        Self::default()
    }

    /// Ingest one datagram. Returns the fully-reassembled message if this
    /// datagram completed one, else `None`. Malformed datagrams are ignored.
    ///
    /// # Panics
    ///
    /// Panics if the internal partial-message map is missing an entry that was
    /// just inserted (an invariant violation that cannot occur in practice).
    pub fn ingest(&mut self, datagram: &[u8]) -> Option<Vec<u8>> {
        if datagram.len() < CHUNK_HEADER_LEN {
            return None;
        }
        let msg_id = u32::from_le_bytes([datagram[0], datagram[1], datagram[2], datagram[3]]);
        let idx = u16::from_le_bytes([datagram[4], datagram[5]]);
        let count = u16::from_le_bytes([datagram[6], datagram[7]]);
        if count == 0 || idx >= count {
            return None;
        }

        let entry = self
            .partial
            .entry(msg_id)
            .or_insert_with(|| PartialMessage {
                count,
                chunks: BTreeMap::new(),
            });
        entry.chunks.insert(idx, datagram[CHUNK_HEADER_LEN..].to_vec());

        if entry.chunks.len() == entry.count as usize {
            let msg = self.partial.remove(&msg_id).unwrap();
            let mut out = Vec::new();
            for (_, chunk) in msg.chunks {
                out.extend_from_slice(&chunk);
            }
            return Some(out);
        }

        // Bound memory: evict the oldest partial message if too many pile up.
        if self.partial.len() > MAX_PARTIAL_MESSAGES {
            if let Some((&oldest, _)) = self.partial.iter().next() {
                self.partial.remove(&oldest);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn chunk_reassemble_roundtrip() {
        let data: Vec<u8> = (0..3000u32).map(|i| u8::try_from(i % 256).unwrap_or(0)).collect();
        let chunks = chunk_message(7, &data, DEFAULT_CHUNK_PAYLOAD);
        assert_eq!(chunks.len(), 3, "3000 bytes / 1200 = 3 chunks");

        let mut r = UdpReassembler::new();
        let mut done = None;
        for c in &chunks {
            if let Some(m) = r.ingest(c) {
                done = Some(m);
            }
        }
        assert_eq!(done.expect("message completes"), data);
    }

    #[test]
    fn reassembles_out_of_order() {
        let data: Vec<u8> = (0..2500u32).map(|i| (i % 7) as u8).collect();
        let mut chunks = chunk_message(1, &data, DEFAULT_CHUNK_PAYLOAD);
        chunks.reverse(); // deliver last-chunk-first

        let mut r = UdpReassembler::new();
        let mut done = None;
        for c in &chunks {
            if let Some(m) = r.ingest(c) {
                done = Some(m);
            }
        }
        assert_eq!(done.expect("reorder-tolerant"), data);
    }

    #[test]
    fn incomplete_message_yields_nothing() {
        let data: Vec<u8> = vec![9u8; 2000]; // 2 chunks
        let chunks = chunk_message(2, &data, DEFAULT_CHUNK_PAYLOAD);
        let mut r = UdpReassembler::new();
        assert!(
            r.ingest(&chunks[0]).is_none(),
            "one of two chunks is not a complete message"
        );
    }

    #[test]
    fn empty_message_roundtrips() {
        let chunks = chunk_message(3, &[], DEFAULT_CHUNK_PAYLOAD);
        assert_eq!(chunks.len(), 1, "empty payload still sends one chunk");
        let mut r = UdpReassembler::new();
        assert_eq!(r.ingest(&chunks[0]).expect("completes"), Vec::<u8>::new());
    }

    #[test]
    fn two_interleaved_messages() {
        let a: Vec<u8> = vec![1u8; 1500]; // 2 chunks
        let b: Vec<u8> = vec![2u8; 1500]; // 2 chunks
        let ca = chunk_message(10, &a, DEFAULT_CHUNK_PAYLOAD);
        let cb = chunk_message(11, &b, DEFAULT_CHUNK_PAYLOAD);
        let mut r = UdpReassembler::new();
        // Interleave: a0, b0, a1 (-> a done), b1 (-> b done)
        assert!(r.ingest(&ca[0]).is_none());
        assert!(r.ingest(&cb[0]).is_none());
        assert_eq!(r.ingest(&ca[1]).expect("a done"), a);
        assert_eq!(r.ingest(&cb[1]).expect("b done"), b);
    }
}
