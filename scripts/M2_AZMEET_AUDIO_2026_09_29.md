# M2 AzMeet audio, mute state, Leave: report (2026-09-29)

Branch `wt/m2-azmeet-audio` (base `d9ce25179`); Worker in azul-apps worktree `/Users/fschutt/Development/azul-apps-m1`,
branch `cf-workers-meet` (nothing on `main`).

## Findings first

- **The output API exists**: `AudioSink::open(AudioConfig) -> AudioSink`, `play(AudioFrame)`, `is_open`,
  `frames_played`, `close` (dll `desktop/extra/audio/mod.rs`; ALSA on Linux, cpal on Windows, AVAudioEngine on
  macOS/iOS, AAudio on Android). So no design stop at item 2. How `play` waits differs: an ALSA write blocks while the
  device buffer is full, AVAudioEngine keeps at most 8 buffers in flight and drops beyond, cpal queues up to 4 s.
- **iroh frames are latest-wins at both ends** (engine.rs): a pending outbound frame is replaced by the next one of its
  track, and the receiver keeps only the newest frame per (peer, track) until `recv`. Messages are reliable and
  ordered. Audio therefore rides a track with redundancy (see below), and mute state rides messages.
- **Headless mocks**: under `AZ_BACKEND=headless` only `AudioDeviceList::enumerate` is mocked. `MicrophoneWidget`'s
  platform backend is registered in the layout pass (`ensure_mic_backend`) and would open the real mic, and
  `AudioSink::open` would open the real output. So the headless test does **not** enable the mic: AzMeet guards itself
  (a headless run uses a 440 Hz tone source and never opens a sink), and `AZMEET_TEST_TONE=1` feeds the tone into the
  send path.

## What was built

### 1. Audio over iroh (`examples/azul-meet/src/audio.rs`, pure, 28 unit tests; glue in `lib.rs`)
- **Wire format** (track 3, `AUDIO_TRACK`): `[version 1][codec 1 = PCM s16][count][0][sample_rate u32]` then per packet
  `[sequence u32][length u16][length x i16]`, little endian, oldest first. A packet is 20 ms of mono at the
  microphone's rate (960 samples at 48 kHz). **Every frame carries the newest packet and the two before it**
  (`REDUNDANCY = 3`), so a frame replaced before it leaves, or overwritten before the UI thread reads it, loses nothing
  while the next arrives. Codec byte 1 = PCM s16; Opus is the planned codec 2.
- **Packetizer**: any chunk length, any channel count (mixed down to mono), clamped PCM16 (NaN = silence); mute resets
  the unfinished packet and the redundancy history, sequence numbers continue; a new rate starts a new packet.
- **JitterBuffer** (one per connection handle): fills to 3 packets (60 ms) before playing; sequence order; duplicates
  (the redundancy) told apart from late packets by a 128-packet seen window; underrun = silence while the missing
  packet keeps its turn; a hole with later packets present = silence and skip; refill after 200 ms of nothing (the peer
  muted); oldest dropped past 10 packets (200 ms); restart on a new rate; `clear()` for deafen.
- **PlayoutClock**: one turn per 20 ms of wall time, skips (max 100 ms catch-up) after a stall instead of bursting.
- **Playout thread** (`azmeet-playout`, started with the first received packet, ends when `MeetState` goes; holds a
  `Weak`): each turn pops every peer's buffer and plays it through **that peer's own `AudioSink`** opened at the peer's
  rate (the OS mixes them). The UI thread only decodes frames and pushes packets under a mutex held for microseconds;
  a blocking sink write only paces the playout thread. Headless: no sink is opened; buffers are drained and counted.
- **Send**: `MicrophoneWidget::on_frame` (device) or the tone (`pump_tone` in the 15 ms pump) -> `send_audio` ->
  `Packetizer` -> `broadcast_frame(AUDIO_TRACK, ...)`; nothing is sent (and the packetizer resets) without peers.
- **Debug line** (devices panel, "Audio" column): `Sending: 440 Hz test tone, 16-bit PCM, 20 ms packets, N so far` and
  per peer `Audio from Ben: 250 packets, 247 played, 3 silent, 0 late, 3 buffered` (`, filling up` while refilling).

### 2. Mute / deafen state
- New **Deafen** button (drops the buffered audio, ignores incoming packets). Every change, and every new connection,
  sends `[1][flags]` (bit 0 muted, bit 1 deafened) with `send_message`; unknown kinds are ignored, extra bytes allowed.
- People list: `Ben · connected · muted`, `Ben · connected · muted · deafened`, `Ada (you) · muted` (nothing appended
  until a peer's first state message).

### 3. Leave
- Worker (azul-apps): `DELETE /rooms/<id>/peers/<node_id>` -> `200 {ok, room, removed}` (idempotent; 404 unknown room or
  a code; 400 `invalid_node_id`; 405 `Allow: DELETE` for every other method on that path, so no cross-origin preflight
  succeeds; no CORS). `store.removePeer(roomId, nodeId)` (one SQL DELETE, both adapters). README updated.
  `node --test "test/*.test.mjs"`: **50 pass** (was 41).
- App: a red **Leave** button (rooms mode) disconnects every peer, forgets their audio, stops announcing and polling,
  sends the DELETE (`on_left` only logs), returns to the start screen with "You left the meeting.". A peer that still
  dials is refused (connections are accepted only in `InRoom` / `Ended`). **Every HTTP request carries its room session**
  (`RoomSession::session`, bumped on enter and leave; `Reply { app, session }` is what the resume callbacks get), so an
  answer from a meeting since left can neither dial nor clear `busy`.

### 4. `two-clients.mjs`
Both clients run with `AZMEET_TEST_TONE=1` (never a real device). After the M1 checks it asserts: each window counts
>= 50 packets (1 s) received and >= 25 played from the other; each app printed "no audio device is opened"; Ben clicks
**Mute** (debug-server `click` op by text) and Ada shows `Ben · connected · muted`; Ben clicks **Leave**, his window
shows the start screen, the dev server stops listing him within 30 s (the DELETE; TTL is 120 s), and Ada's window stops
listing him. The connected check now accepts an audio suffix and skips the `Ben · waiting for video` tile.
Dry run against a Node stand-in for the app (scratchpad `m2/fake-azmeet.mjs`, which also exercises the Worker's new
DELETE through the dev server): **PASS**. The Rust app itself was not run (no compiling here).

### 5. Guide
`doc/guide/en/system/realtime-media.md` rewritten around AzMeet (rooms and the Worker routes, video, the audio path,
mute / deafen / leave, run it, env vars, the headless rule and the test, what is not there yet), plus corrected
backend facts ("What is on-device"). `doc/guide/en/data/networking.md`: two sentences (receive-side latest-wins; what
azul-meet does now). Not an owned file of another task as far as I know.

## Commits

azul, `wt/m2-azmeet-audio`:
- `916764657` test(azmeet): how audio travels between participants (RED)
- `6aa0ba788` feat(azmeet): packets, jitter buffer, playout clock and mute state for audio
- `0379cf912` test(azmeet): two AzMeet processes hear each other, see a mute, and one leaves (RED)
- `6b2843642` feat(azmeet): audio between participants, mute and deafen state, and Leave
- `165e73eb9` docs(guide): realtime media describes AzMeet's rooms, audio and Leave
- plus `docs(m2): progress` commits and this report

azul-apps, `cf-workers-meet` (worktree `/Users/fschutt/Development/azul-apps-m1`):
- `97aced2` test(meet): a participant can leave a room at once (RED)
- `1b8ef59` feat(meet): DELETE /rooms/<id>/peers/<node_id>, so a participant leaves at once

## api.json

**No change.** Existing API used: `AudioSink::open / is_open / play`, `AudioConfig`, `AudioFrame` (struct literal),
`F32Vec: From<Vec<f32>>` (Rust binding), `IrohEndpoint::broadcast_frame / send_message / disconnect`, `IrohEvent`
fields, `IrohEventKind::Message`, `HttpMethod::Delete`, `callbacks::CallbackType`.

Recommended library follow-up (not done, it is outside this task's files): mock capture and playback under the armed
request store the way `AudioDeviceList::enumerate` is - `MicrophoneWidget` feeds its test tone and `AudioSink::open`
returns a counting stub when `AZ_E2E` / `AZ_BACKEND=headless` - so apps need no guard of their own.

## Least sure to compile

`audio.rs` type-checks alone (`rustc --edition 2021 --crate-type lib --test --emit=metadata`, no warnings).
`lib.rs` cannot be compiled here; the new glue was type-checked in a harness (scratchpad `m2/harness.rs`, built by
`make_harness.py`): stubs with the generated signatures (`AzRef`/`AzRefMut` guards with `Drop` borrowed from
`&'a mut self`, `Into<..>` generics, `AzCallbackInfo: Copy`) plus 45 items copied verbatim from lib.rs
(`playout_loop`, `receive_audio`, `pump_tone`, `leave_meeting`, `on_leave`, `mic_on_frame`, `deafen_toggle`,
`reply_parts`, `on_left`, `apply_link_event`, `roster`, `audio_lines`, ...): no errors, no warnings. Not covered by the
harness, so least sure:
1. `http_thread`: building `Reply { app: i.app.clone(), session: i.job.session }` inside the `downcast_ref().map(..)`
   closure (same shape as M1's tuple).
2. `on_room_opened` / `on_announced` / `on_peers`: `let Some((mut data, session)) = reply_parts(data) else { .. }`
   shadowing the parameter, then M1's unchanged body.
3. `pump_link`: `receive_audio(&mut s, &event)` with `s` an `AzRefMut` guard (same as M1's `apply_link_event` call).
4. `call_layout`: fn items (`mic_toggle`, `on_leave`, ...) passed as `CallbackType` to `toolbar_button`.
5. `lock(shared)` where `shared: &mut Arc<Mutex<Playout>>` (deref coercion into `&Mutex<T>` with `T` inferred) - this
   one is in the harness and passes.

Runtime assumptions to watch in the E2E: the debug server's `click` op by `text` works on a headless window (it clicks
the parent of the first text node containing "Mute" / "Leave"); the toolbar is inside the 1100x720 window.

## Test commands for the parent

```sh
# the Worker (green here: 50)
cd /Users/fschutt/Development/azul-apps-m1/cf-workers/meet && node --test "test/*.test.mjs"

# AzMeet unit tests: audio.rs (28) + rooms.rs (15)
cargo test -p AzMeet --lib

# build AzMeet and libazul with the debug server, then
node examples/azul-meet/scripts/two-clients.mjs \
  --worker-dir /Users/fschutt/Development/azul-apps-m1/cf-workers/meet \
  [--bin <path to AzMeet>] [--timeout 90] [--keep-logs]
```

By hand (real devices): dev server, then two `AZMEET_WORKER=http://127.0.0.1:8787 AZMEET_NAME=.. AzMeet`, New meeting
/ paste / Join, both **Unmute mic**: each hears the other; Mute shows on the other side; Leave returns to the start
screen. Headphones, or expect echo (no echo cancellation yet).

## What is left

1. **Codec**: Opus as codec 2 (about 32 kbit/s instead of 768, 2.3 Mbit/s with the repeats; in-band FEC would replace
   the 3x redundancy; its PLC would replace plain silence). Needs a codec in the dll (libopus or a Rust port) and an
   api.json surface (`AudioEncoder` / `AudioDecoder`, like `VideoEncoder`).
2. **Echo cancellation, noise suppression, AGC**: nothing yet (the design doc points at `sonora` / `aec3`, AEC3 ports);
   AEC needs the playout signal as the far-end reference, which argues for doing it in the dll next to `AudioSink`.
3. **Transport**: a datagram or "every frame" track mode in `IrohEndpoint` (no latest-wins) would drop the redundancy;
   audio priority over video (the design doc's QoS rule) is not set (frames share the default stream priority).
4. **Playout clock**: pushed at wall-clock pace; `AudioSink` has no "queued frames" query or pull callback, so device
   clock drift over a long call is only bounded by the sink's own caps (AVAudioEngine drops past 8 buffers; cpal queues
   up to 4 s; ALSA self-paces). A pull-style sink (or `queued_frames()`) would fix it.
5. **Library mocks for headless** (see api.json section).
6. **Load balancer**: `IrohLoadBalancer` still unwired - full mesh, every peer sends to every peer.
7. **Video codecs**: still MJPEG 320x180; H.264 / HEVC / AV1 with keyframe requests on loss, simulcast renditions per
   tile size, native encoders.
8. **Worker**: signed announcements and a signed leave (today anyone holding the room id can announce or remove any
   node id); deploy; `azlin://` registration.
9. Mic and speaker still unverified on real hardware on every OS; the in-process demo plays each window's audio in the
   other (echo when both unmute).
