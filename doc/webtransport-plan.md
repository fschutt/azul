# Real-Time "Chat Room" Media Transport for azul — `AzWebTransport` Design

Status: proposal / design doc · Date: 2026-06-18
Scope: replace the crude `Udp` API with a typed `AzWebTransport` API powering a
Google-Meet-style demo ("azul-meet") across native (desktop) and wasm (browser).

> Distilled from a cited research pass (WebRTC vs WebTransport vs MoQ). Sources at the end.

---

## 0. Recommendation (TL;DR)

**Build `AzWebTransport` on WebTransport (HTTP/3 over QUIC), not WebRTC**, using the
`web-transport` crate family — `web-transport-quinn` natively, `web-transport-wasm` in the
browser — behind a `webtransport-native` feature flag (mirroring azul's `video-native`).

The fact that flips the historical default: **WebTransport reached cross-browser Baseline in
March 2026** (Safari 26.4 shipped it on by default; Chrome since v97/2022, Firefox v114/2023).
The classic "WebRTC is the only thing that runs everywhere (esp. iOS)" reason is gone.

Why this is right *specifically for azul*:
1. **azul already owns the media pipeline** (Camera/Screen/Mic widgets capture, `VideoEncoder`
   encodes, `AudioSink` plays). WebRTC's value is exactly that pipeline — adopting it means
   fighting it to reach raw frames. WebTransport wants what azul produces: raw encoded bytes.
2. **The C-ABI / no-async-FFI constraint kills WebRTC.** WebRTC = SDP offer/answer + ICE +
   DTLS-SRTP state machine; exposing that over a flat `{ptr, run_destructor}` handle with no
   async is a multi-month nightmare. WebTransport = "open a URL, write bytes on streams/datagrams"
   — maps ~1:1 onto azul's existing `Udp` handle + worker-thread pattern.
3. **Client–server matches the requirement.** WebTransport is strictly client→server; we *want*
   a coordination server that fans out (never browser P2P). A WebTransport fan-out server is a few
   hundred LOC of `wtransport`/`quinn`; a WebRTC SFU is not — and there is **no production-grade
   turnkey Rust SFU** in 2026 (str0m/webrtc-rs are libraries; mature SFUs are C++/Go/Kotlin).
4. **No heavy C++ link.** Mature native WebRTC = Google's `libwebrtc` (LiveKit `webrtc-sys`), a
   large C++ dep. `quinn` is pure Rust and cross-compiles like the rest of azul.
5. **One codebase, native + wasm.** `web-transport` presents one API over `web-transport-quinn`
   (native) and `web-transport-wasm` (browser `web-sys`) — write the engine once.

**The price, stated plainly:** WebTransport gives transport, not a media stack. You lose what
WebRTC gives free — media-aware congestion control (GCC/TWCC), the jitter buffer, NACK/FEC, and
AEC/NS/AGC — and must build/borrow them. For a v1 demo on a LAN / decent network this is fine if
we're honest. **v1 ships without real bandwidth estimation**; it leans on QUIC's own per-stream
congestion control + pacing, plus three cheap tricks (stale-frame stream reset, a fixed audio
playout buffer, a keyframe-request control message) and a stats getter so the app throttles its
own encoder.

**Migration path:** our wire framing mirrors **Media-over-QUIC (MoQ)** semantics
(tracks/groups/objects). MoQ (IETF draft-18, May 2026; Rust: Cloudflare `moq-rs`, `moq-dev/moq`)
is the emerging pub/sub media layer over WebTransport (relays, caching, priority-drop). Adopt it
later by swapping the hand-rolled framing for `moq-lite` **without changing the public API**.

---

## 1. What must be sent (azul-meet)

To a coordination server that fans out to other room members: **camera video** (encoded H.264/
VP8/AV1, keyframes+deltas), **screen-share video** (second track), **mic audio**, **chat** (text),
**system/control** (mute, request-keyframe, raise-hand). And receive the same back, tagged by
sender. The removed `Udp` (`azul_dll::unified::udp::Udp`, poll `recv()->OptionU8Vec`) is the
template for the *shape* and the floor for *quality* (raw UDP, no crypto, lossy chunking).
`AzWebTransport` keeps the ergonomics, fixes the substance (QUIC/TLS1.3, real reliability when
wanted, a real server, typed messages).

---

## 2. WebRTC vs WebTransport vs MoQ (summary)

- **WebRTC** — a *complete* media stack: SRTP tracks, built-in codecs (VP8/H.264 CB MTI; Opus),
  getUserMedia/getDisplayMedia capture, RTCDataChannel (SCTP) for chat, GCC/TWCC congestion
  control, jitter buffer, NACK/FEC, ICE/STUN/TURN NAT traversal, and a signaling burden you build
  yourself. Groups need an SFU (mediasoup/Janus/LiveKit/Jitsi — none turnkey in Rust). ~96%
  browser support. Its strengths are exactly what azul already has → poor fit.
- **WebTransport** — a transport primitive over HTTP/3/QUIC: **unidirectional streams**
  (reliable+ordered), **bidirectional streams**, and **datagrams** (unreliable, unordered,
  congestion-controlled, ~1200-byte floor, never fragmented/retransmitted). No head-of-line
  blocking across streams. Client = just a URL (no SDP/ICE/signaling). Strictly client–server.
  No media-aware CC, no jitter buffer, no codecs/capture (bring azul's). ~85% browser, Baseline
  Mar 2026.
- **MoQ** — pub/sub live media over QUIC/WebTransport (Objects→Groups→Tracks), relay fan-out,
  caching, priority-drop. Pre-RFC (don't depend on it for v1), but shape our framing to swap in
  `moq-lite` later.

### Comparison

| Dimension | WebRTC | WebTransport (pick) | MoQ (later) |
|---|---|---|---|
| Layer | full media stack | transport (streams+datagrams) | media pub/sub over WT/QUIC |
| Client API over C-ABI | huge (SDP/ICE/DTLS) | tiny (URL + read/write bytes) | small (pub/sub tracks) |
| Congestion control | GCC/TWCC built-in | QUIC stream CC only | priority-drop + relay |
| Jitter/NACK/FEC | built-in | **you build it** | partial |
| Codecs/capture | built-in | bring your own (azul has) | bring your own |
| Topology | P2P mesh / SFU | client–server fan-out | relay/CDN |
| NAT traversal | ICE/STUN/TURN | none (it's a URL) | none |
| Server (Rust) | no turnkey SFU | ~few hundred LOC | `moq-relay` exists |
| Native Rust maturity | libwebrtc (C++) / alpha | `quinn`/`wtransport` (pure Rust) | early |
| Browser reach 2026 | ~96% (2017) | ~85% (Baseline Mar 2026) | rides WT |
| Fit with azul | fights azul's pipeline | matches it | matches it |

---

## 3. Carrying each kind at the right quality

Physical constraints: a QUIC datagram must fit ~1200 B and is never retransmitted; encoded video
frames are far larger → **video rides streams**; raw f32 PCM (20 ms mono 48 kHz = 3840 B) doesn't
fit a datagram → audio must be **Opus-compressed (~50–150 B/20 ms)** to ride a datagram, else fall
back to a reliable stream.

| Kind | Size/rate | Loss tol. | Latency | v1 transport | Rationale |
|---|---|---|---|---|---|
| Video keyframe | 10s–100s KB | low | medium | reliable uni-stream, one per frame | must arrive intact; own stream avoids HoL-blocking other tracks |
| Video delta | KBs | medium | high | uni-stream per frame, cancellable (`RESET_STREAM` if stale) | drop a late frame, don't stall newer ones |
| Audio | Opus ~50–150 B/20 ms | high | very high | **datagram** (Opus) | small, frequent, useless if late; raw-f32 fallback on a stream (LAN) |
| Chat | small text | none | low | reliable ordered stream | must arrive in order |
| System/control | small | none | low–med | reliable ordered stream | state must be consistent |

Key choices: **one stream per video frame** (mirrors MoQ group/object; per-frame success/cancel);
**keyframe re-request as a control message** (PLI/FIR analog) on join / after loss; the
reliability knob is per-call so the app can override.

**Congestion/jitter reality (v1 stub):** rely on QUIC per-stream CC + pacing (datagrams ride the
connection CC, dropped under loss = correct for audio). Implement cheaply: (1) stale-frame
`RESET_STREAM`; (2) fixed audio playout buffer ~60–100 ms; (3) keyframe-request control msg;
(4) a `stats()` getter (rtt/cwnd/queue from quinn) so the app throttles its own `VideoEncoder`.
Defer: real bandwidth estimation (GCC), simulcast/SVC, FEC, adaptive jitter, and AEC/NS/AGC
(the last is a *capture* concern for `MicrophoneWidget`, independent of transport).

---

## 4. Coordination server

A **byte-level fan-out relay** (a "dumb SFU"): terminates one WebTransport session per
participant, reads each one's streams/datagrams, re-emits to every *other* member in the room with
the sender's `peer_id` stamped in. No transcoding, no server-side jitter buffer. Room membership
(room id → peers); `PeerJoined`/`PeerLeft`. The human "meeting link"
`https://meet.example.com/r/AbCdEf` maps to a WebTransport URL
`https://host:4433/r/AbCdEf?t=<token>` — room id in the path, capability token (HMAC/JWT) in the
query (validated before accepting the session). Topology = **server star fan-out** (each client
holds one connection); video fan-out cost grows ~N² at the server uplink → limit forwarded video
to active speakers / pinned tiles, then move to MoQ relays for scale.

```rust
// wtransport on quinn — ~150 LOC is the WHOLE "signaling + SFU" surface (no SDP/ICE/DTLS)
use std::collections::HashMap; use std::sync::Arc;
use tokio::sync::{mpsc, Mutex}; use wtransport::{Endpoint, Identity, ServerConfig};
type PeerId = u64; type RoomId = String;
#[derive(Clone)] enum Out { Stream { reliable: bool, bytes: Vec<u8> }, Datagram(Vec<u8>) }
#[derive(Default)] struct Room { peers: HashMap<PeerId, mpsc::UnboundedSender<Out>> }
type Rooms = Arc<Mutex<HashMap<RoomId, Room>>>;

#[tokio::main] async fn main() -> anyhow::Result<()> {
    let identity = Identity::self_signed(["localhost","meet.example.com"])?; // dev; print cert SHA-256
    let config = ServerConfig::builder().with_bind_default(4433).with_identity(identity).build();
    let server = Endpoint::server(config)?; let rooms: Rooms = Default::default();
    let mut next_id: PeerId = 1;
    loop {
        let incoming = server.accept().await; let rooms = rooms.clone();
        let peer_id = next_id; next_id += 1;
        tokio::spawn(async move {
            let req = incoming.await?; let path = req.path().to_string(); // "/r/AbCdEf?t=..."
            let (room_id, token) = parse_path(&path);
            if !auth_ok(&room_id,&token) { req.forbidden().await; return Ok::<(),anyhow::Error>(()); }
            let conn = req.accept().await?;                       // WebTransport session up
            let (tx, mut rx) = mpsc::unbounded_channel::<Out>();
            register(&rooms,&room_id,peer_id,tx.clone()).await;
            broadcast(&rooms,&room_id,peer_id, Out::Stream{reliable:true,bytes:frame_peer_joined(peer_id)}).await;
            let conn_w = conn.clone();
            let writer = tokio::spawn(async move { while let Some(out)=rx.recv().await { match out {
                Out::Datagram(b)=>{ let _=conn_w.send_datagram(b); }
                Out::Stream{bytes,..}=>{ if let Ok(mut s)=conn_w.open_uni().await { let _=s.write_all(&bytes).await; let _=s.finish().await; } }
            }}});
            loop { tokio::select! {
                dg = conn.receive_datagram() => { let mut b=dg?.to_vec(); stamp_peer_id(&mut b,peer_id);
                    broadcast(&rooms,&room_id,peer_id, Out::Datagram(b)).await; }
                s = conn.accept_uni() => { let mut s=s?; let mut buf=Vec::new(); s.read_to_end(&mut buf).await?;
                    stamp_peer_id(&mut buf,peer_id); let reliable=is_reliable_kind(&buf);
                    broadcast(&rooms,&room_id,peer_id, Out::Stream{reliable,bytes:buf}).await; }
                else => break,
            }}
            unregister(&rooms,&room_id,peer_id).await;
            broadcast(&rooms,&room_id,peer_id, Out::Stream{reliable:true,bytes:frame_peer_left(peer_id)}).await;
            writer.abort(); Ok(())
        });
    }
}
```

---

## 5. Sequencing

1. **Now:** WebTransport datagrams + streams with our thin framing (§7). Ship the **stub engine
   first** (compiles everywhere; loopback echoes your own sends back as a synthetic peer → build
   the azul-meet UI with zero server), then the real `webtransport-native` engine, then wasm.
2. **Later:** swap framing for `moq-lite` (priority-drop, relays, caching) — public API unchanged.
   Do NOT take a pre-RFC dependency in v1.

The stub (feature off, and initial wasm): identical `#[repr(C)]` layout; `connect` returns a
handle whose engine is a **loopback** (echoes sends as a synthetic peer) so the UI is testable.

---

## 6. `AzWebTransport` C-ABI API

api.json module **`misc`** (next to the outgoing `Udp`),
`external = azul_dll::unified::webtransport::WebTransport`. Delivery = **poll-recv** (like `Udp`):
a bg thread owns the QUIC connection + pushes events into a queue; the app drains via a `Timer`.

### 6.1 Surface (Rust-API view)
```rust
WebTransport::connect(url: String, room: String, token: String) -> WebTransport
WebTransport::is_connected(&self) -> bool
WebTransport::stats(&self) -> WtStats
WebTransport::send_video(&self, track_id: u32, frame: U8Vec, is_keyframe: bool, quality: WtReliability) -> bool
WebTransport::send_audio(&self, track_id: u32, frame: AudioFrame, quality: WtReliability) -> bool
WebTransport::send_chat(&self, text: String) -> bool
WebTransport::send_system(&self, data: U8Vec) -> bool
WebTransport::request_keyframe(&self, peer_id: u64, track_id: u32) -> bool  // PLI/FIR analog
WebTransport::recv(&self) -> OptionWtEvent                                  // poll, drain each tick
WebTransport::close(&mut self)
```
`quality: WtReliability` is the per-call knob (defaults: keyframe→ReliableOrdered, delta→Datagram
or cancellable stream, audio→Datagram, chat/system→ReliableOrdered). `send_audio` takes the raw
`AudioFrame`; the engine Opus-encodes for datagrams (raw-f32-over-stream fallback) so callers learn
no codec.

### 6.2 Receive event — flat "fat" struct (binding-friendly; unused fields default per kind)
```rust
#[repr(C, u8)] pub enum WtEventKind {
    Connected, Disconnected, PeerJoined, PeerLeft, Video, Audio, Chat, System,
}
#[repr(C)] pub struct WtEvent {
    pub kind: WtEventKind,
    pub peer_id: u64,        // 0 for Connected/Disconnected
    pub track_id: u32,       // video/audio only
    pub is_keyframe: bool,   // video only
    pub text: AzString,      // chat / peer name / disconnect reason
    pub audio: AudioFrame,   // Audio only — pass straight to AudioSink::play
    pub data: U8Vec,         // video/system bytes — pass straight to VideoDecoder
}
#[repr(C, u8)] pub enum WtReliability { ReliableOrdered, ReliableUnordered, Datagram }
#[repr(C)] pub struct WtStats {        // from quinn; the throttling seam
    pub rtt_us: u64, pub cwnd_bytes: u64, pub send_queue_bytes: u64,
    pub bytes_sent: u64, pub bytes_recv: u64, pub packet_loss_x1000: u32,
}
```
Reusing `AudioFrame`/`U8Vec` means `recv()` hands you something you pass straight to
`AudioSink::play(ev.audio)` / `VideoDecoder(ev.data)`.

### 6.3 api.json shape (follows Udp / AudioSink conventions)
```json
"WebTransport": {
  "external": "azul_dll::unified::webtransport::WebTransport",
  "doc": ["Real-time room transport over WebTransport (HTTP/3 / QUIC).",
          "Poll recv() from a Timer; send typed media/chat/control.",
          "Engine runs on a background thread; no network when the native feature is off."],
  "custom_impls": ["Clone", "Default", "Drop"],
  "struct_fields": [ { "ptr": { "type": "c_void", "ref_kind": "mutptr" },
                       "run_destructor": { "type": "bool" } } ],
  "constructors": { "connect": {
      "fn_args": [ { "url": "String" }, { "room": "String" }, { "token": "String" } ],
      "fn_body": "azul_dll::unified::webtransport::WebTransport::connect(url, room, token)" } },
  "functions": {
    "is_connected": { "fn_args": [ {"self":"ref"} ], "returns": {"type":"bool"}, "fn_body": "object.is_connected()" },
    "stats":        { "fn_args": [ {"self":"ref"} ], "returns": {"type":"WtStats"}, "fn_body": "object.stats()" },
    "send_video":   { "fn_args": [ {"self":"ref"},{"track_id":"u32"},{"frame":"U8Vec"},{"is_keyframe":"bool"},{"quality":"WtReliability"} ],
                      "returns": {"type":"bool"}, "fn_body": "object.send_video(track_id, frame, is_keyframe, quality)" },
    "send_audio":   { "fn_args": [ {"self":"ref"},{"track_id":"u32"},{"frame":"AudioFrame"},{"quality":"WtReliability"} ],
                      "returns": {"type":"bool"}, "fn_body": "object.send_audio(track_id, frame, quality)" },
    "send_chat":    { "fn_args": [ {"self":"ref"},{"text":"String"} ], "returns": {"type":"bool"}, "fn_body": "object.send_chat(text)" },
    "send_system":  { "fn_args": [ {"self":"ref"},{"data":"U8Vec"} ], "returns": {"type":"bool"}, "fn_body": "object.send_system(data)" },
    "request_keyframe": { "fn_args": [ {"self":"ref"},{"peer_id":"u64"},{"track_id":"u32"} ], "returns": {"type":"bool"}, "fn_body": "object.request_keyframe(peer_id, track_id)" },
    "recv":         { "fn_args": [ {"self":"ref"} ], "returns": {"type":"OptionWtEvent"}, "fn_body": "object.recv()" },
    "close":        { "fn_args": [ {"self":"refmut"} ], "fn_body": "object.close()" }
  },
  "repr": "C"
}
```
Plus `WtReliability`, `WtEventKind`, `WtEvent`, `WtStats` (module `misc`) and `OptionWtEvent`
(module `option`, mirroring `OptionU8Vec`). Generated C symbols + azul.h + all bindings fall out of
`azul-doc codegen all`; `dll/cargo test` memtest verifies every `Az*` size/alignment.

### 6.4 Rust inner sketch (stub first; real engine behind a feature)
`dll/src/desktop/extra/webtransport/mod.rs`: the `#[repr(C)] WebTransport { ptr, run_destructor }`
handle with the AudioSink-style Clone/Default/Drop; a boxed `WtInner { cmd_tx, evt_rx, connected,
stats, join }`; `connect` spawns the engine thread; sends enqueue `WtCmd`s; `recv()` does a
non-blocking `try_recv` → `OptionWtEvent`.
```rust
// STUB (no webtransport-native, + initial wasm): loopback so the UI is testable
#[cfg(not(feature="webtransport-native"))]
fn spawn_engine(_u:String,_r:String,_t:String, cmd_rx:mpsc::Receiver<WtCmd>, evt_tx:mpsc::Sender<WtEvent>,
                connected:Arc<AtomicBool>,_s:Arc<Mutex<WtStats>>) -> JoinHandle<()> {
  std::thread::spawn(move || {
    connected.store(true,Ordering::Relaxed);
    let _=evt_tx.send(WtEvent::connected());
    let _=evt_tx.send(WtEvent::peer_joined(999,"Loopback".into()));
    for cmd in cmd_rx { match cmd {                       // echo our sends back as a fake peer
      WtCmd::Close=>break,
      WtCmd::Chat(t)=>{ let _=evt_tx.send(WtEvent::chat(999,t)); }
      WtCmd::Video{track_id,data,keyframe,..}=>{ let _=evt_tx.send(WtEvent::video(999,track_id,keyframe,data)); }
      WtCmd::Audio{track_id,frame,..}=>{ let _=evt_tx.send(WtEvent::audio(999,track_id,frame)); }
      WtCmd::System(d)=>{ let _=evt_tx.send(WtEvent::system(999,d)); }
      WtCmd::RequestKeyframe{..}=>{}
    }}
    connected.store(false,Ordering::Relaxed);
  })
}
// REAL native engine: std::thread -> tokio current-thread runtime -> web_transport_quinn::connect(url),
// three async loops (outbound drain cmd_rx; inbound accept_uni; inbound datagrams), quinn stats -> WtStats.
```
`dll/src/unified/webtransport.rs` re-exports the native impl (desktop) or a repr-C-identical wasm
stub (`web-transport-wasm` fast-follow). `dll/Cargo.toml`:
`webtransport-native = ["dep:web-transport-quinn","dep:tokio","dep:opus"]`, added to `build-dll`.

### 6.5 — see Wire format (§7).

---

## 7. Wire format (thin, MoQ-shaped)

Every message starts with a fixed **16-byte little-endian header** so one parser serves streams +
datagrams and the server fans out by rewriting one field:
```
off sz field
0   1  version (=1)
1   1  kind    (1=Audio 2=Video 3=Chat 4=System 5=Control)
2   1  flags   (bit0=keyframe; bit1=opus)
3   1  reserved
4   4  track_id (u32)   -- 0 for chat/system
8   8  peer_id  (u64)   -- client sends 0; server stamps the real sender
16  .. payload
```
- **Video:** kind=2, one uni-stream per frame; header + encoded bytes; `finish()` = boundary;
  stale deltas dropped via `RESET_STREAM`.
- **Audio:** kind=1, datagram, flags|opus; payload = `sample_rate(u32) channels(u16)` + Opus packet
  (raw-f32 fallback over a ReliableUnordered stream).
- **Chat/System/Control:** kind=3/4/5, reliable ordered stream; control sub-types are a 1-byte tag
  at payload start (`RequestKeyframe`, `PeerJoined`, `PeerLeft`).

Isomorphic to MoQ Objects (frame) / Groups (GOP) / Tracks (`track_id`) → the migration replaces
this section with `moq-lite`, leaving §6 intact.

---

## 8. v1 implementation checklist

1. **api.json:** add `WebTransport`, `WtReliability`, `WtEventKind`, `WtEvent`, `WtStats` (module
   `misc`) + `OptionWtEvent` (module `option`). **Remove the `Udp` class.**
2. **Module skeleton:** `dll/src/desktop/extra/webtransport/mod.rs` (handle + `WtInner` + stub
   engine) + `dll/src/unified/webtransport.rs` (native / wasm stub); register in
   `dll/src/unified/mod.rs`. Copy the `{ptr,run_destructor}` Clone/Default/Drop from `AudioSink`.
3. **Stub engine first:** loopback echo → build azul-meet UI with no server. Verify `cargo build`
   native + wasm32.
4. **Wire format:** the 16-byte header + per-kind framing as a small pure module in `azul-core`
   (alongside/superseding `core/src/udp_framing.rs`), unit-tested.
5. **Real native engine** behind `webtransport-native` (`web-transport-quinn` + `tokio`): connect
   (URL `/r/<room>?t=<token>`, dev self-signed cert hash), 3 async loops, QUIC stats → `WtStats`.
6. **Audio codec:** Opus encode/decode (`opus` crate or dlopen libopus, azul-style); raw-f32-over-
   stream fallback.
7. **Cheap adaptivity:** stale-frame `RESET_STREAM`; receiver fixed audio playout buffer; wire
   `request_keyframe` to the local encoder.
8. **Coordination server:** the `wtransport` fan-out (§4) — room map, token auth, peer-id stamping,
   PeerJoined/Left, datagram + uni-stream forwarding. Ship a `gen-meeting-link` helper printing
   `https://…/r/<id>?t=<hmac>` + the dev cert SHA-256.
9. **azul-meet demo:** Camera/Screen widgets → `VideoEncoder` → `send_video`; Mic (`OnAudioFrame`)
   → `send_audio`; chat box → `send_chat`; a `Timer` drains `recv()` → route Video→VideoDecoder→
   tile, Audio→AudioSink, Chat→UI, PeerJoined/Left→roster.
10. **wasm engine** (fast-follow): swap the wasm stub for `web-transport-wasm`
    (`--cfg=web_sys_unstable_apis`); same `WtInner` shape, browser tasks + `RefCell` queue.
11. **Codegen + verify:** `azul-doc normalize` → `codegen all` → `dll cargo test` (memtest) →
    `cargo build -p azul-dll --features build-dll`.
12. **Document the gap:** README note — v1 has NO media-aware congestion control / FEC / AEC;
    production needs real bandwidth estimation (or the MoQ migration) + capture-side AEC/NS/AGC.

**Deferred (post-v1):** real congestion control driving encoder bitrate; simulcast/SVC; MoQ
migration (`moq-lite`/`moq-rs`) for priority-drop + relays; capture-side AEC/NS/AGC; relay chaining
+ active-speaker video selection for large rooms.

---

## Sources

WebTransport/QUIC: MDN WebTransport API; Chrome web.dev WebTransport; W3C TR/webtransport
(2026-03-25); draft-ietf-webtrans-http3-15; RFC 9000/9114/9221/9220; caniuse/webtransport; WebKit
Safari 26.4 notes; webrtc.ventures "WebTransport is now Baseline"; webrtchacks WebCodecs+
WebTransport. WebRTC: RFC 8827/7742/7874/8831/8445/8489/8656; MDN data-channels + signaling;
draft-ietf-rmcat-gcc-02; WebRTC For The Curious; mediasoup/Janus/LiveKit/Pion/str0m/webrtc-rs/
livekit rust-sdks; webrtchacks TURN usage; caniuse getDisplayMedia. MoQ/Rust: IETF MoQ WG;
draft-ietf-moq-transport-18; draft-lcurley-moq-lite-04; Cloudflare moq-rs; moq-dev/moq;
moq.dev "replacing WebRTC"; quinn; wtransport; web-transport(-quinn/-wasm); h3; s2n-quic;
web-sys WebTransport.
