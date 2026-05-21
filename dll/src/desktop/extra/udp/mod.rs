//! UDP transport handle (SUPER_PLAN_2 P8) - `Udp`.
//!
//! The fault-tolerant packet-sharing primitive for azul-meet: a thin,
//! non-blocking wrapper over `std::net::UdpSocket`, exposed as a C-ABI handle
//! (`ptr` + `run_destructor`) like `Db` / `AudioSink` / the `Pdf` handle. The
//! app holds it in its own State (no globals) and pumps it from a timer or a
//! background thread: `recv()` to drain incoming datagrams, `send_to(...)` to
//! push one out.
//!
//! Two layers:
//!
//! - **Byte-level** (`send_to` / `recv`): one datagram in, one datagram out.
//!   The app frames its own small payload (an audio chunk, a chat line). Fits
//!   in a single datagram (<= ~1200 bytes is always safe).
//! - **Chunked** (`send_chunked` / `recv_chunked`): for payloads larger than a
//!   datagram (a video keyframe), splits the message into sequenced chunks and
//!   reassembles them on the far side. UDP's loss/reorder is tolerated: a
//!   message whose chunks never all arrive is simply dropped (no retransmit,
//!   no head-of-line blocking) - exactly the model realtime A/V wants.
//!
//! `std::net` is always available, so - unlike the rodio/sqlite/printpdf
//! engines - there is no feature gate and no stub: this is real on every
//! target.

use core::ffi::c_void;
use std::collections::BTreeMap;
use std::net::UdpSocket;

use azul_css::{AzString, OptionU8Vec, U8Vec};

/// Max single UDP datagram we'll receive into (64 KiB - the IPv4 ceiling).
const RECV_BUF_LEN: usize = 65_536;
/// Chunk header: msg_id (u32 LE) + chunk_idx (u16 LE) + chunk_count (u16 LE).
const CHUNK_HEADER_LEN: usize = 8;
/// Conservative per-datagram payload for chunked sends (leaves room under a
/// typical ~1400-byte path MTU, so chunks don't fragment on the wire).
const CHUNK_PAYLOAD_LEN: usize = 1200;
/// Cap on in-flight partial messages; the oldest is evicted past this, so a
/// lost chunk can never leak memory.
const MAX_PARTIAL_MESSAGES: usize = 256;

/// A message being reassembled from chunks.
struct PartialMessage {
    chunk_count: u16,
    chunks: BTreeMap<u16, Vec<u8>>,
}

/// Engine-side state behind the `Udp` handle: the socket plus the chunking
/// send counter + reassembly buffers.
struct UdpInner {
    socket: UdpSocket,
    next_msg_id: u32,
    partial: BTreeMap<u32, PartialMessage>,
}

/// A non-blocking UDP socket handle. Open with [`Udp::bind`], then
/// [`send_to`](Self::send_to) / [`recv`](Self::recv) for single datagrams, or
/// [`send_chunked`](Self::send_chunked) / [`recv_chunked`](Self::recv_chunked)
/// for larger messages. C-ABI handle convention (`run_destructor` + custom
/// `Drop`) like `Db`.
#[repr(C)]
pub struct Udp {
    /// Opaque pointer to the engine-side `UdpInner` (or null when not bound /
    /// on failure).
    pub ptr: *mut c_void,
    /// Whether this handle owns (and on drop closes) the socket.
    pub run_destructor: bool,
}

impl Clone for Udp {
    fn clone(&self) -> Self {
        // Non-owning shallow handle copy - only the original closes the socket
        // (the FFI handle convention).
        Udp {
            ptr: self.ptr,
            run_destructor: false,
        }
    }
}

impl Default for Udp {
    fn default() -> Self {
        Udp {
            ptr: core::ptr::null_mut(),
            run_destructor: false,
        }
    }
}

impl Udp {
    /// Bind a UDP socket to `local_addr` (e.g. "0.0.0.0:0" for any interface +
    /// an OS-assigned port, or "0.0.0.0:9000" for a fixed port). The socket is
    /// set non-blocking so [`recv`](Self::recv) never stalls the UI thread.
    /// Returns an invalid handle (`is_open()` false) on failure.
    pub fn bind(local_addr: AzString) -> Udp {
        match UdpSocket::bind(local_addr.as_str()) {
            Ok(socket) => {
                let _ = socket.set_nonblocking(true);
                let inner = Box::new(UdpInner {
                    socket,
                    next_msg_id: 0,
                    partial: BTreeMap::new(),
                });
                Udp {
                    ptr: Box::into_raw(inner) as *mut c_void,
                    run_destructor: true,
                }
            }
            Err(_) => Udp::default(),
        }
    }

    /// Whether the socket bound successfully.
    pub fn is_open(&self) -> bool {
        !self.ptr.is_null()
    }

    fn inner(&self) -> Option<&mut UdpInner> {
        unsafe { (self.ptr as *mut UdpInner).as_mut() }
    }

    /// Send one datagram to `remote_addr` (e.g. "192.168.1.5:9000"). Returns
    /// the number of bytes sent (`0` on failure / not open). The payload is the
    /// app's own framing; keep it under ~1200 bytes, or use
    /// [`send_chunked`](Self::send_chunked).
    pub fn send_to(&self, remote_addr: AzString, data: U8Vec) -> usize {
        match self.inner() {
            Some(i) => i
                .socket
                .send_to(data.as_ref(), remote_addr.as_str())
                .unwrap_or(0),
            None => 0,
        }
    }

    /// Receive one pending datagram (non-blocking). `Some(bytes)` if one was
    /// waiting, else `None`. Poll from a timer or background thread.
    pub fn recv(&self) -> OptionU8Vec {
        match self.inner() {
            Some(i) => {
                let mut buf = vec![0u8; RECV_BUF_LEN];
                match i.socket.recv(&mut buf) {
                    Ok(n) => {
                        buf.truncate(n);
                        OptionU8Vec::Some(U8Vec::from_vec(buf))
                    }
                    Err(_) => OptionU8Vec::None,
                }
            }
            None => OptionU8Vec::None,
        }
    }

    /// Send a (possibly large) message to `remote_addr`, split into sequenced
    /// chunks. Use this for payloads bigger than a datagram (a video frame).
    /// Reassemble on the far side with [`recv_chunked`](Self::recv_chunked).
    /// Returns the number of chunks sent (`0` if not open). Lossy by nature:
    /// if a chunk is dropped in flight the message is discarded by the
    /// receiver rather than stalling.
    pub fn send_chunked(&self, remote_addr: AzString, data: U8Vec) -> usize {
        let i = match self.inner() {
            Some(i) => i,
            None => return 0,
        };
        let msg_id = i.next_msg_id;
        i.next_msg_id = i.next_msg_id.wrapping_add(1);

        let bytes = data.as_ref();
        let count = if bytes.is_empty() {
            1
        } else {
            bytes.len().div_ceil(CHUNK_PAYLOAD_LEN)
        };
        let count_u16 = count.min(u16::MAX as usize) as u16;
        let addr = remote_addr.as_str();
        let mut sent = 0usize;
        for idx in 0..count_u16 {
            let start = idx as usize * CHUNK_PAYLOAD_LEN;
            let end = (start + CHUNK_PAYLOAD_LEN).min(bytes.len());
            let mut packet = Vec::with_capacity(CHUNK_HEADER_LEN + (end - start));
            packet.extend_from_slice(&msg_id.to_le_bytes());
            packet.extend_from_slice(&idx.to_le_bytes());
            packet.extend_from_slice(&count_u16.to_le_bytes());
            if start < end {
                packet.extend_from_slice(&bytes[start..end]);
            }
            if i.socket.send_to(&packet, addr).is_ok() {
                sent += 1;
            }
        }
        sent
    }

    /// Drain pending datagrams and return the next fully-reassembled chunked
    /// message, or `None` if none completed this poll. Out-of-order chunks are
    /// buffered; messages whose chunks never all arrive are eventually evicted
    /// (bounded memory). Pair with [`send_chunked`](Self::send_chunked).
    pub fn recv_chunked(&self) -> OptionU8Vec {
        let i = match self.inner() {
            Some(i) => i,
            None => return OptionU8Vec::None,
        };
        let mut buf = vec![0u8; RECV_BUF_LEN];
        loop {
            let n = match i.socket.recv(&mut buf) {
                Ok(n) if n >= CHUNK_HEADER_LEN => n,
                Ok(_) => continue, // runt / malformed, skip
                Err(_) => return OptionU8Vec::None, // WouldBlock or error: done for now
            };
            let msg_id = u32::from_le_bytes([buf[0], buf[1], buf[2], buf[3]]);
            let idx = u16::from_le_bytes([buf[4], buf[5]]);
            let count = u16::from_le_bytes([buf[6], buf[7]]);
            if count == 0 || idx >= count {
                continue;
            }
            let entry = i.partial.entry(msg_id).or_insert_with(|| PartialMessage {
                chunk_count: count,
                chunks: BTreeMap::new(),
            });
            entry.chunks.insert(idx, buf[CHUNK_HEADER_LEN..n].to_vec());

            if entry.chunks.len() == entry.chunk_count as usize {
                // Complete: concatenate chunks in index order.
                let msg = i.partial.remove(&msg_id).unwrap();
                let mut out = Vec::new();
                for (_, chunk) in msg.chunks {
                    out.extend_from_slice(&chunk);
                }
                return OptionU8Vec::Some(U8Vec::from_vec(out));
            }

            // Bound memory: evict the oldest partial message if too many pile up.
            if i.partial.len() > MAX_PARTIAL_MESSAGES {
                if let Some((&oldest, _)) = i.partial.iter().next() {
                    i.partial.remove(&oldest);
                }
            }
        }
    }

    /// The local address the socket is bound to (e.g. to learn the OS-assigned
    /// port after binding to ":0"). Empty string if not open.
    pub fn local_addr(&self) -> AzString {
        self.inner()
            .and_then(|i| i.socket.local_addr().ok())
            .map(|a| AzString::from_string(a.to_string()))
            .unwrap_or(AzString::from_const_str(""))
    }

    /// Close the socket + release it. (Dropping the handle does this too.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut UdpInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

impl Drop for Udp {
    fn drop(&mut self) {
        self.drop_inner();
    }
}
