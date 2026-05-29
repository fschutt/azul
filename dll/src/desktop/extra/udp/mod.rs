//! UDP transport handle (SUPER_PLAN_2 P8) - `Udp`.
//!
//! The fault-tolerant packet-sharing primitive for azul-meet: a thin,
//! non-blocking wrapper over `std::net::UdpSocket`, exposed as a C-ABI handle
//! (`ptr` + `run_destructor`) like `Db` / `AudioSink` / the `Pdf` handle. The
//! app holds it in its own State (no globals) and pumps it from a timer or a
//! background thread.
//!
//! Two layers:
//!
//! - **Byte-level** (`send_to` / `recv`): one datagram in, one datagram out.
//!   The app frames its own small payload. Fits in a single datagram
//!   (<= ~1200 bytes is always safe).
//! - **Chunked** (`send_chunked` / `recv_chunked`): for payloads larger than a
//!   datagram (a video keyframe). The chunking + reassembly is the pure,
//!   unit-tested [`azul_core::udp_framing`] logic; UDP's loss/reorder is
//!   tolerated (a message whose chunks never all arrive is dropped, no
//!   retransmit) - the model realtime A/V wants.
//!
//! `std::net` is always available, so - unlike the rodio/sqlite/printpdf
//! engines - there is no feature gate and no stub: this is real on every
//! target.

use core::ffi::c_void;
use std::net::UdpSocket;

use azul_core::udp_framing::{chunk_message, UdpReassembler, DEFAULT_CHUNK_PAYLOAD};
use azul_css::{AzString, OptionU8Vec, U8Vec};

/// Max single UDP datagram we'll receive into (64 KiB - the IPv4 ceiling).
const RECV_BUF_LEN: usize = 65_536;

/// Engine-side state behind the `Udp` handle: the socket plus the chunked-send
/// counter + the reassembly buffer.
struct UdpInner {
    socket: UdpSocket,
    next_msg_id: u32,
    reassembler: UdpReassembler,
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
                crate::plog_info!("[udp] bound {}", local_addr.as_str());
                let _ = socket.set_nonblocking(true);
                let inner = Box::new(UdpInner {
                    socket,
                    next_msg_id: 0,
                    reassembler: UdpReassembler::new(),
                });
                Udp {
                    ptr: Box::into_raw(inner) as *mut c_void,
                    run_destructor: true,
                }
            }
            Err(_) => {
                crate::plog_warn!("[udp] bind {} failed", local_addr.as_str());
                Udp::default()
            }
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
    /// chunks via [`azul_core::udp_framing`]. Use this for payloads bigger than
    /// a datagram (a video frame); reassemble on the far side with
    /// [`recv_chunked`](Self::recv_chunked). Returns the number of chunks sent
    /// (`0` if not open). Lossy by nature: a dropped chunk discards the message
    /// at the receiver rather than stalling.
    pub fn send_chunked(&self, remote_addr: AzString, data: U8Vec) -> usize {
        let i = match self.inner() {
            Some(i) => i,
            None => return 0,
        };
        let msg_id = i.next_msg_id;
        i.next_msg_id = i.next_msg_id.wrapping_add(1);
        let addr = remote_addr.as_str();
        let mut sent = 0usize;
        for packet in chunk_message(msg_id, data.as_ref(), DEFAULT_CHUNK_PAYLOAD) {
            if i.socket.send_to(&packet, addr).is_ok() {
                sent += 1;
            }
        }
        sent
    }

    /// Drain pending datagrams and return the next fully-reassembled chunked
    /// message, or `None` if none completed this poll. Out-of-order chunks are
    /// buffered; incomplete messages are dropped (bounded memory). Pair with
    /// [`send_chunked`](Self::send_chunked).
    pub fn recv_chunked(&self) -> OptionU8Vec {
        let i = match self.inner() {
            Some(i) => i,
            None => return OptionU8Vec::None,
        };
        let mut buf = vec![0u8; RECV_BUF_LEN];
        loop {
            match i.socket.recv(&mut buf) {
                Ok(n) => {
                    if let Some(msg) = i.reassembler.ingest(&buf[..n]) {
                        return OptionU8Vec::Some(U8Vec::from_vec(msg));
                    }
                    // otherwise keep draining until a message completes / WouldBlock
                }
                Err(_) => return OptionU8Vec::None, // WouldBlock or error: done for now
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
