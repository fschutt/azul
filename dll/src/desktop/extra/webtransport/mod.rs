//! WebTransport room-transport handle — `WebTransport` (replaces the old `Udp`).
//!
//! The real-time "chat room" primitive for azul-meet: send camera/screen video,
//! microphone audio, chat and system messages to a coordination server that fans
//! them out to other participants, and receive the same back tagged by sender.
//!
//! C-ABI handle convention (`ptr` + `run_destructor`, custom Clone/Default/Drop)
//! like `Udp` / `AudioSink`. Delivery is **poll-based** (`recv()` like `Udp`): a
//! background engine thread owns the connection and pushes [`WtEvent`]s into a
//! queue the app drains from a timer.
//!
//! v1 ships a **stub/loopback engine** (echoes the caller's own sends back as a
//! synthetic peer, id 999) so the azul-meet UI can be built and tested with no
//! server. The real WebTransport (HTTP/3 / QUIC via `web-transport-quinn`) engine
//! lands behind a `webtransport-native` feature — see `doc/webtransport-plan.md`.

use core::ffi::c_void;
use core::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Mutex};
use std::thread::JoinHandle;

use azul_core::audio::AudioFrame;
use azul_css::{AzString, F32Vec, U8Vec};

// ---------------------------------------------------------------------------
// POD types (the wire-facing API surface)
// ---------------------------------------------------------------------------

/// Per-send reliability / quality knob.
#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
pub enum WtReliability {
    /// QUIC stream, in order (chat, control, video keyframes).
    ReliableOrdered,
    /// QUIC stream, no cross-message ordering (raw-audio fallback, bulk).
    ReliableUnordered,
    /// Unreliable, unordered datagram (audio, droppable video deltas).
    Datagram,
}

impl Default for WtReliability {
    fn default() -> Self {
        WtReliability::ReliableOrdered
    }
}

/// Discriminant for [`WtEvent`].
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

/// Connection stats for app-side throttling (sourced from QUIC; all zero for the
/// v1 stub engine).
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

/// One inbound event from the room. Inspect `kind`; only the relevant fields are
/// set (others are default/empty). `audio` is ready for `AudioSink::play`; `data`
/// (for `Video`) is the encoded frame, ready for a `VideoDecoder`.
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

impl WtEvent {
    fn empty(kind: WtEventKind) -> WtEvent {
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
    pub fn connected() -> WtEvent {
        WtEvent::empty(WtEventKind::Connected)
    }
    pub fn disconnected(reason: AzString) -> WtEvent {
        let mut e = WtEvent::empty(WtEventKind::Disconnected);
        e.text = reason;
        e
    }
    pub fn peer_joined(peer_id: u64, name: AzString) -> WtEvent {
        let mut e = WtEvent::empty(WtEventKind::PeerJoined);
        e.peer_id = peer_id;
        e.text = name;
        e
    }
    pub fn peer_left(peer_id: u64) -> WtEvent {
        let mut e = WtEvent::empty(WtEventKind::PeerLeft);
        e.peer_id = peer_id;
        e
    }
    pub fn video(peer_id: u64, track_id: u32, is_keyframe: bool, data: U8Vec) -> WtEvent {
        let mut e = WtEvent::empty(WtEventKind::Video);
        e.peer_id = peer_id;
        e.track_id = track_id;
        e.is_keyframe = is_keyframe;
        e.data = data;
        e
    }
    pub fn audio(peer_id: u64, track_id: u32, frame: AudioFrame) -> WtEvent {
        let mut e = WtEvent::empty(WtEventKind::Audio);
        e.peer_id = peer_id;
        e.track_id = track_id;
        e.audio = frame;
        e
    }
    pub fn chat(peer_id: u64, text: AzString) -> WtEvent {
        let mut e = WtEvent::empty(WtEventKind::Chat);
        e.peer_id = peer_id;
        e.text = text;
        e
    }
    pub fn system(peer_id: u64, data: U8Vec) -> WtEvent {
        let mut e = WtEvent::empty(WtEventKind::System);
        e.peer_id = peer_id;
        e.data = data;
        e
    }
}

/// Optional [`WtEvent`] returned by [`WebTransport::recv`] (`None` when nothing
/// is queued).
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

// ---------------------------------------------------------------------------
// engine command + boxed state
// ---------------------------------------------------------------------------

enum WtCmd {
    Video {
        track_id: u32,
        data: Vec<u8>,
        keyframe: bool,
        mode: WtReliability,
    },
    Audio {
        track_id: u32,
        frame: AudioFrame,
        mode: WtReliability,
    },
    Chat(String),
    System(Vec<u8>),
    RequestKeyframe {
        peer_id: u64,
        track_id: u32,
    },
    Close,
}

/// Engine-side state behind the handle: the command channel to the engine
/// thread, the event channel back, liveness flag, stats, and the join handle.
struct WtInner {
    cmd_tx: mpsc::Sender<WtCmd>,
    evt_rx: Mutex<mpsc::Receiver<WtEvent>>,
    connected: Arc<AtomicBool>,
    stats: Arc<Mutex<WtStats>>,
    join: Option<JoinHandle<()>>,
}

// ---------------------------------------------------------------------------
// the handle
// ---------------------------------------------------------------------------

/// A room-transport connection. Open with [`connect`](Self::connect), poll
/// [`recv`](Self::recv) from a timer, and `send_*` your media/chat/control.
#[repr(C)]
pub struct WebTransport {
    /// Opaque pointer to the engine-side `WtInner` (null when not connected).
    pub ptr: *mut c_void,
    /// Whether this handle owns (and on drop closes) the connection.
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
    fn drop(&mut self) {
        self.drop_inner();
    }
}

impl WebTransport {
    /// Connect to a coordination server, joining `room` with capability `token`.
    /// `url` is the server origin (e.g. "https://host:4433"); the handle resolves
    /// "{url}/r/{room}?t={token}". Spawns the engine thread immediately; watch
    /// [`is_connected`](Self::is_connected) / drain [`recv`](Self::recv) for the
    /// `Connected` event.
    pub fn connect(url: AzString, room: AzString, token: AzString) -> WebTransport {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (evt_tx, evt_rx) = mpsc::channel();
        let connected = Arc::new(AtomicBool::new(false));
        let stats = Arc::new(Mutex::new(WtStats::default()));
        let join = spawn_engine(
            url.as_str().to_string(),
            room.as_str().to_string(),
            token.as_str().to_string(),
            cmd_rx,
            evt_tx,
            connected.clone(),
            stats.clone(),
        );
        let inner = Box::new(WtInner {
            cmd_tx,
            evt_rx: Mutex::new(evt_rx),
            connected,
            stats,
            join: Some(join),
        });
        WebTransport {
            ptr: Box::into_raw(inner) as *mut c_void,
            run_destructor: true,
        }
    }

    fn inner(&self) -> Option<&WtInner> {
        unsafe { (self.ptr as *const WtInner).as_ref() }
    }

    /// Whether the engine reports an established session.
    pub fn is_connected(&self) -> bool {
        self.inner()
            .map_or(false, |i| i.connected.load(Ordering::Relaxed))
    }

    /// A snapshot of connection stats (the app-side throttling seam).
    pub fn stats(&self) -> WtStats {
        self.inner()
            .map(|i| *i.stats.lock().unwrap())
            .unwrap_or_default()
    }

    /// Send one encoded video frame on `track_id`. `is_keyframe` flags an
    /// independently-decodable frame. Returns false if not connected.
    pub fn send_video(
        &self,
        track_id: u32,
        frame: U8Vec,
        is_keyframe: bool,
        quality: WtReliability,
    ) -> bool {
        self.inner().map_or(false, |i| {
            i.cmd_tx
                .send(WtCmd::Video {
                    track_id,
                    data: frame.as_ref().to_vec(),
                    keyframe: is_keyframe,
                    mode: quality,
                })
                .is_ok()
        })
    }

    /// Send one audio frame on `track_id`. Returns false if not connected.
    pub fn send_audio(&self, track_id: u32, frame: AudioFrame, quality: WtReliability) -> bool {
        self.inner().map_or(false, |i| {
            i.cmd_tx
                .send(WtCmd::Audio {
                    track_id,
                    frame,
                    mode: quality,
                })
                .is_ok()
        })
    }

    /// Send a chat message (reliable, ordered).
    pub fn send_chat(&self, text: AzString) -> bool {
        self.inner().map_or(false, |i| {
            i.cmd_tx
                .send(WtCmd::Chat(text.as_str().to_string()))
                .is_ok()
        })
    }

    /// Send an opaque system/control message (reliable, ordered).
    pub fn send_system(&self, data: U8Vec) -> bool {
        self.inner().map_or(false, |i| {
            i.cmd_tx.send(WtCmd::System(data.as_ref().to_vec())).is_ok()
        })
    }

    /// Ask `peer_id` to emit a keyframe on `track_id` (the PLI/FIR analog).
    pub fn request_keyframe(&self, peer_id: u64, track_id: u32) -> bool {
        self.inner().map_or(false, |i| {
            i.cmd_tx
                .send(WtCmd::RequestKeyframe { peer_id, track_id })
                .is_ok()
        })
    }

    /// Receive the next queued inbound event (non-blocking). Poll from a timer.
    pub fn recv(&self) -> OptionWtEvent {
        match self.inner() {
            Some(i) => match i.evt_rx.lock().unwrap().try_recv() {
                Ok(ev) => OptionWtEvent::Some(ev),
                Err(_) => OptionWtEvent::None,
            },
            None => OptionWtEvent::None,
        }
    }

    /// Close the connection + release it. (Dropping the handle does this too.)
    pub fn close(&mut self) {
        if let Some(i) = self.inner() {
            let _ = i.cmd_tx.send(WtCmd::Close);
        }
        self.drop_inner();
    }

    fn drop_inner(&mut self) {
        if self.run_destructor && !self.ptr.is_null() {
            unsafe {
                drop(Box::from_raw(self.ptr as *mut WtInner));
            }
        }
        self.ptr = core::ptr::null_mut();
        self.run_destructor = false;
    }
}

// ---------------------------------------------------------------------------
// v1 stub / loopback engine
//
// Echoes the caller's own sends back as if from a synthetic peer (id 999), so
// the azul-meet UI is fully testable with no server. The real engine
// (web-transport-quinn) replaces this behind the `webtransport-native` feature.
// ---------------------------------------------------------------------------

fn spawn_engine(
    _url: String,
    _room: String,
    _token: String,
    cmd_rx: mpsc::Receiver<WtCmd>,
    evt_tx: mpsc::Sender<WtEvent>,
    connected: Arc<AtomicBool>,
    _stats: Arc<Mutex<WtStats>>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        connected.store(true, Ordering::Relaxed);
        let _ = evt_tx.send(WtEvent::connected());
        let _ = evt_tx.send(WtEvent::peer_joined(999, AzString::from_const_str("Loopback")));
        for cmd in cmd_rx {
            let ev = match cmd {
                WtCmd::Close => break,
                WtCmd::Chat(t) => WtEvent::chat(999, AzString::from_string(t)),
                WtCmd::Video {
                    track_id,
                    data,
                    keyframe,
                    ..
                } => WtEvent::video(999, track_id, keyframe, U8Vec::from_vec(data)),
                WtCmd::Audio {
                    track_id, frame, ..
                } => WtEvent::audio(999, track_id, frame),
                WtCmd::System(d) => WtEvent::system(999, U8Vec::from_vec(d)),
                WtCmd::RequestKeyframe { .. } => continue,
            };
            if evt_tx.send(ev).is_err() {
                break;
            }
        }
        connected.store(false, Ordering::Relaxed);
    })
}
