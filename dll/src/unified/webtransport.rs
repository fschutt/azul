//! Unified `WebTransport` handle. See [`crate::unified`].
//!
//! Native re-exports the real engine; wasm gets a repr-C-identical no-op stub
//! (no QUIC/thread backend yet — the `web-transport-wasm` engine is a follow-up).
//! The wasm POD types MUST stay byte-compatible with
//! `crate::desktop::extra::webtransport` (the codegen memtest checks the native
//! side; keep the two in sync).

#[cfg(all(feature = "cabi_internal", not(target_arch = "wasm32")))]
pub use crate::desktop::extra::webtransport::*;

#[cfg(target_arch = "wasm32")]
mod wasm_stub {
    use core::ffi::c_void;

    use azul_core::audio::AudioFrame;
    use azul_css::{AzString, F32Vec, U8Vec};

    #[repr(C)]
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub enum WtReliability {
        ReliableOrdered,
        ReliableUnordered,
        Datagram,
    }
    impl Default for WtReliability {
        fn default() -> Self {
            WtReliability::ReliableOrdered
        }
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
    pub enum WtEventKind {
        Connected,
        Disconnected,
        PeerJoined,
        PeerLeft,
        Video,
        Audio,
        Chat,
        System,
    }
    impl Default for WtEventKind {
        fn default() -> Self {
            WtEventKind::Disconnected
        }
    }

    #[repr(C)]
    #[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Default)]
    pub struct WtStats {
        pub rtt_us: u64,
        pub cwnd_bytes: u64,
        pub send_queue_bytes: u64,
        pub bytes_sent: u64,
        pub bytes_recv: u64,
        pub packet_loss_x1000: u32,
    }

    #[repr(C)]
    #[derive(Debug, Clone)]
    pub struct WtEvent {
        pub kind: WtEventKind,
        pub peer_id: u64,
        pub track_id: u32,
        pub is_keyframe: bool,
        pub text: AzString,
        pub audio: AudioFrame,
        pub data: U8Vec,
    }

    #[repr(C, u8)]
    #[derive(Debug, Clone)]
    pub enum OptionWtEvent {
        None,
        Some(WtEvent),
    }
    impl Default for OptionWtEvent {
        fn default() -> Self {
            OptionWtEvent::None
        }
    }

    /// wasm no-op stub of the desktop `WebTransport` handle.
    #[repr(C)]
    pub struct WebTransport {
        pub ptr: *mut c_void,
        pub run_destructor: bool,
    }
    impl Clone for WebTransport {
        fn clone(&self) -> Self {
            WebTransport {
                ptr: self.ptr,
                run_destructor: false,
            }
        }
    }
    impl Default for WebTransport {
        fn default() -> Self {
            WebTransport {
                ptr: core::ptr::null_mut(),
                run_destructor: false,
            }
        }
    }
    impl Drop for WebTransport {
        fn drop(&mut self) {}
    }
    impl WebTransport {
        pub fn connect(_url: AzString, _room: AzString, _token: AzString) -> WebTransport {
            WebTransport::default()
        }
        pub fn is_connected(&self) -> bool {
            false
        }
        pub fn stats(&self) -> WtStats {
            WtStats::default()
        }
        pub fn send_video(
            &self,
            _track_id: u32,
            _frame: U8Vec,
            _is_keyframe: bool,
            _quality: WtReliability,
        ) -> bool {
            false
        }
        pub fn send_audio(&self, _track_id: u32, _frame: AudioFrame, _quality: WtReliability) -> bool {
            false
        }
        pub fn send_chat(&self, _text: AzString) -> bool {
            false
        }
        pub fn send_system(&self, _data: U8Vec) -> bool {
            false
        }
        pub fn request_keyframe(&self, _peer_id: u64, _track_id: u32) -> bool {
            false
        }
        pub fn recv(&self) -> OptionWtEvent {
            OptionWtEvent::None
        }
        pub fn close(&mut self) {}
    }

    // Keep the empty-frame helper available to anything that builds a WtEvent.
    impl WtEvent {
        pub fn empty(kind: WtEventKind) -> WtEvent {
            WtEvent {
                kind,
                peer_id: 0,
                track_id: 0,
                is_keyframe: false,
                text: AzString::from_const_str(""),
                audio: AudioFrame {
                    sample_rate: 0,
                    channels: 0,
                    samples: F32Vec::from_vec(Vec::new()),
                },
                data: U8Vec::from_vec(Vec::new()),
            }
        }
    }
}

#[cfg(target_arch = "wasm32")]
pub use wasm_stub::*;
