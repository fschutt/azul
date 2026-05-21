//! azul-meet - the P8 goal app from SUPER_PLAN_2.
//!
//! The full realtime-audio stack, end-to-end on the public API:
//!
//!   MicrophoneWidget (capture) --on_frame--> Udp.send_to
//!   Udp.recv --recv Timer--> AudioSink.play
//!
//! Wired as a UDP **loopback** (binds 127.0.0.1:0 and sends to its own
//! OS-assigned port) so the whole capture -> serialize -> UDP -> deserialize ->
//! playback round-trip runs on one machine with no peer. A real two-party
//! connection is the same code with `peer` set to the remote address; video +
//! large-frame chunking land in follow-up ticks.
//!
//! No globals: the `Udp` + `AudioSink` handles live in the app's own State
//! (a RefAny), reached by the mic hook + the recv Timer.

use azul::dom::OnAudioFrameCallback;
use azul::misc::{AudioConfig, Udp};
use azul::option::OptionU8Vec;
use azul::prelude::*;
use azul::str::String as AzString;
use azul::task::TerminateTimer;
use azul::vec::{F32Vec, U8Vec};
use azul::widgets::{AudioFrame, MicrophoneWidget};
use azul::window::AudioSink;

/// App state. The transport + playback handles live here (no globals); the mic
/// hook + recv Timer reach them through the dataset RefAny.
struct MeetState {
    /// Bound UDP socket (loopback to our own port for this demo).
    udp: Udp,
    /// Audio output the received frames are played to.
    sink: AudioSink,
    /// Where captured frames are sent (our own bound address, for loopback).
    peer: AzString,
    /// Whether the socket bound successfully.
    connected: bool,
    /// Frames captured + sent.
    sent: usize,
    /// Frames received + played.
    received: usize,
}

// --- AudioFrame <-> bytes: the app's own UDP framing (small format: rate,
// channels, then interleaved f32 samples, all little-endian). ---

/// Build an FFI `U8Vec` from a byte slice (the binding copies from a ptr + len;
/// there is no `from_vec`).
fn vec_u8(v: &[u8]) -> U8Vec {
    if v.is_empty() {
        U8Vec::create()
    } else {
        U8Vec::copy_from_ptr(&v[0], v.len())
    }
}

/// Build an FFI `F32Vec` from an f32 slice.
fn vec_f32(v: &[f32]) -> F32Vec {
    if v.is_empty() {
        F32Vec::create()
    } else {
        F32Vec::copy_from_ptr(&v[0], v.len())
    }
}

fn frame_to_bytes(f: &AudioFrame) -> U8Vec {
    let mut b: Vec<u8> = Vec::new();
    b.extend_from_slice(&f.sample_rate.to_le_bytes());
    b.extend_from_slice(&f.channels.to_le_bytes());
    for s in f.samples.as_ref() {
        b.extend_from_slice(&s.to_le_bytes());
    }
    vec_u8(&b)
}

fn bytes_to_frame(b: &[u8]) -> Option<AudioFrame> {
    if b.len() < 6 {
        return None;
    }
    let sample_rate = u32::from_le_bytes([b[0], b[1], b[2], b[3]]);
    let channels = u16::from_le_bytes([b[4], b[5]]);
    let mut samples: Vec<f32> = Vec::new();
    let mut i = 6;
    while i + 4 <= b.len() {
        samples.push(f32::from_le_bytes([b[i], b[i + 1], b[i + 2], b[i + 3]]));
        i += 4;
    }
    Some(AudioFrame {
        sample_rate,
        channels,
        samples: vec_f32(&samples),
    })
}

/// Mic `on_frame`: serialize the captured chunk + send it to the peer (the
/// azul-meet send seam).
extern "C" fn on_mic_frame(mut data: RefAny, _info: CallbackInfo, frame: AudioFrame) -> Update {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if s.connected {
            let bytes = frame_to_bytes(&frame);
            let peer = s.peer.clone();
            if s.udp.send_to(peer, bytes) > 0 {
                s.sent += 1;
            }
        }
    }
    Update::RefreshDom
}

/// Recv Timer tick: drain every pending datagram, deserialize, and play it (the
/// receive seam).
extern "C" fn recv_tick(mut data: RefAny, _info: TimerCallbackInfo) -> TimerCallbackReturn {
    if let Some(mut s) = data.downcast_mut::<MeetState>() {
        if s.connected {
            loop {
                match s.udp.recv() {
                    OptionU8Vec::Some(ref bytes) => {
                        if let Some(frame) = bytes_to_frame(bytes.as_ref()) {
                            s.sink.play(frame);
                            s.received += 1;
                        }
                    }
                    OptionU8Vec::None => break,
                }
            }
        }
    }
    TimerCallbackReturn {
        should_update: Update::DoNothing,
        should_terminate: TerminateTimer::Continue,
    }
}

/// Window-create: install the recv Timer that drains the socket each frame.
extern "C" fn startup(data: RefAny, mut info: CallbackInfo) -> Update {
    info.add_timer(
        TimerId::unique(),
        Timer::create(
            data.clone(),
            TimerCallback {
                cb: recv_tick,
                ctx: OptionRefAny::None,
            },
            info.get_system_time_fn(),
        ),
    );
    Update::DoNothing
}

extern "C" fn layout(mut data: RefAny, _info: LayoutCallbackInfo) -> Dom {
    let (connected, peer, sent, received) = match data.downcast_ref::<MeetState>() {
        Some(s) => (s.connected, s.peer.as_str().to_string(), s.sent, s.received),
        None => return Dom::create_body(),
    };

    let status = if connected {
        format!(
            "Connected (UDP loopback {}). Captured + sent {} chunks, received + played {}.",
            peer, sent, received
        )
    } else {
        "UDP socket failed to bind.".to_string()
    };

    let mut body = Dom::create_body()
        .with_child(Dom::create_text("azul-meet (P8) - realtime audio over UDP"))
        .with_child(Dom::create_text(status.as_str()));

    if connected {
        // The microphone widget captures audio; its on_frame hook sends each
        // chunk over UDP. The recv Timer plays back whatever arrives (here, our
        // own loopback). This is the full capture -> UDP -> playback stack.
        let mic = MicrophoneWidget::create(AudioConfig {
            sample_rate: 48_000,
            channels: 1,
        })
        .with_on_frame(
            data.clone(),
            OnAudioFrameCallback {
                cb: on_mic_frame,
                callable: OptionRefAny::None,
            },
        )
        .dom();
        body = body.with_child(mic);
    }

    body
}

fn main() {
    let udp = Udp::bind(AzString::from("127.0.0.1:0"));
    let peer = udp.local_addr(); // loopback: send to our own bound port
    let connected = udp.is_open();
    let sink = AudioSink::open(AudioConfig {
        sample_rate: 48_000,
        channels: 1,
    });

    let state = MeetState {
        udp,
        sink,
        peer,
        connected,
        sent: 0,
        received: 0,
    };

    let data = RefAny::new(state);
    let config = AppConfig::create();
    let app = App::create(data, config);
    let mut window = WindowCreateOptions::create(layout);
    window.create_callback = Some(Callback::create(startup)).into();
    app.run(window);
}
