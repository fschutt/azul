//! UDP transport handle (SUPER_PLAN_2 P8) - `Udp`.
//!
//! The fault-tolerant packet-sharing primitive for azul-meet: a thin,
//! non-blocking wrapper over `std::net::UdpSocket`, exposed as a C-ABI handle
//! (`ptr` + `run_destructor`) like `Db` / `AudioSink` / the `Pdf` handle. The
//! app holds it in its own State (no globals) and pumps it from a timer or a
//! background thread: `recv()` to drain incoming datagrams, `send_to(...)` to
//! push one out.
//!
//! Byte-level on purpose: the app serializes its own payload (an `AudioFrame`
//! it just captured, a chunk of an encoded `VideoFrame`, a chat line) into the
//! `U8Vec` and frames it however it likes. UDP is connectionless + lossy by
//! design - dropped / reordered datagrams are expected, which is exactly the
//! "fault-tolerant" model for realtime A/V (no head-of-line blocking). Larger-
//! than-MTU payloads (e.g. a full video keyframe) need app-side chunking +
//! sequence numbers; that framing layer rides on top of this primitive.
//!
//! `std::net` is always available, so - unlike the rodio/sqlite/printpdf
//! engines - there is no feature gate and no stub: this is real on every
//! target.

use core::ffi::c_void;
use std::net::UdpSocket;

use azul_css::{AzString, OptionU8Vec, U8Vec};

/// Max single UDP datagram we'll receive into (64 KiB - the IPv4 ceiling).
const RECV_BUF_LEN: usize = 65_536;

/// A non-blocking UDP socket handle. Open with [`Udp::bind`], then
/// [`send_to`](Self::send_to) / [`recv`](Self::recv). Carries an OS socket, so
/// it follows the C-ABI handle convention (`run_destructor` + custom `Drop`)
/// like `Db`.
#[repr(C)]
pub struct Udp {
    /// Opaque pointer to the engine-side `std::net::UdpSocket` (or null when
    /// not bound / on failure).
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
            Ok(sock) => {
                let _ = sock.set_nonblocking(true);
                Udp {
                    ptr: Box::into_raw(Box::new(sock)) as *mut c_void,
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

    /// Send one datagram to `remote_addr` (e.g. "192.168.1.5:9000"). Returns
    /// the number of bytes sent (`0` on failure / not open). The payload is the
    /// app's own framing.
    pub fn send_to(&self, remote_addr: AzString, data: U8Vec) -> usize {
        match unsafe { (self.ptr as *const UdpSocket).as_ref() } {
            Some(sock) => sock
                .send_to(data.as_ref(), remote_addr.as_str())
                .unwrap_or(0),
            None => 0,
        }
    }

    /// Receive one pending datagram (non-blocking). Returns `Some(bytes)` if a
    /// datagram was waiting, or `None` if the queue is empty / the socket isn't
    /// open. Poll this from a timer or a background thread.
    pub fn recv(&self) -> OptionU8Vec {
        match unsafe { (self.ptr as *const UdpSocket).as_ref() } {
            Some(sock) => {
                let mut buf = vec![0u8; RECV_BUF_LEN];
                match sock.recv(&mut buf) {
                    Ok(n) => {
                        buf.truncate(n);
                        OptionU8Vec::Some(U8Vec::from_vec(buf))
                    }
                    // WouldBlock (no datagram) or a transient error -> nothing this poll.
                    Err(_) => OptionU8Vec::None,
                }
            }
            None => OptionU8Vec::None,
        }
    }

    /// The local address the socket is bound to (e.g. to learn the OS-assigned
    /// port after binding to ":0", so a peer can be told where to send). Empty
    /// string if not open.
    pub fn local_addr(&self) -> AzString {
        unsafe { (self.ptr as *const UdpSocket).as_ref() }
            .and_then(|s| s.local_addr().ok())
            .map(|a| AzString::from_string(a.to_string()))
            .unwrap_or(AzString::from_const_str(""))
    }

    /// Close the socket + release it. (Dropping the handle does this too;
    /// `close` is for explicit / FFI control.)
    pub fn close(&mut self) {
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut UdpSocket));
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
