# VIDEO8 - progress (branch wt/video8, base 45c6bf98b)

Task: the video leftovers (scripts/waves/wave8/PLAN.md "VIDEO8"). Owns
dll/src/desktop/extra/video_codec/*, stream.rs, the NV12 / colour paths.

## Audit (2026-10-03): the backlog text predates MEET2 (2026-10-01)

Already fixed in this base, RED test + fix each (verified by reading the code):
- stream.rs applies `VideoConfig::output_format`: 16b1ca839 / 1b77f874d
  (`open_session_decoder`, test `a_video_decoder_hands_frames_out_in_the_configs_output_format`).
- colour matrix by value: VT `CFEqual` dlsym'd (`copy_decoded`), AVFoundation `*value == **k601`
  (camera + screen share share `publish_pixel_buffer`): 0cab24e08 / 77ae56342 (+ MEET2 a79c61e21).
- encoder tags / range follow the frames (`session_source`, '420f' / Rec.601 sessions): 77ae56342.
- decoder output change applies at the next picture (`OutputStage`, VTPixelTransferSession), not
  the next IDR: 77ae56342.
- encode / decode off the UI thread (`CodecThread`): 1655f2ab7 / 4b9ce10e6.
- cut_frame keeps the buffer's room (`mem::replace(buf, Vec::with_capacity(room))`): d2c07bafa
  (capture_common.rs:955).
- zero copy step 1 (`NativePicture`, retained IOSurface-backed CVPixelBuffer): 469cbde55 / 1ff6b14db.

Open (the "larger items"): bitrate adaptation, Opus, JPEG fallback off Apple (no H.264 encoder on
Linux / Windows / Android), echo cancellation, zero copy steps 2-3 (GPU binding, encode from the
capture's CVPixelBuffer).

## DONE
- 04b28ef68 docs(video8): progress file; audit of the backlog items
- 7fbcd0e8d test(video): an encoder told a lower bitrate mid-stream spends it from then on (RED)
  (vt_tests `an_encoder_told_a_lower_bitrate_mid_stream_spends_it_from_then_on`)
- 1c5307ac5 feat(video): VideoEncoder::set_bitrate (atomic read by the codec thread before each
  frame; VtEncoder::set_bitrate / bitrate_kbps; helper average_bitrate_bps; wasm stub)
- a26dcd77d test(azmeet): rate.rs RateControl tests (RED)
- af6559f7b feat(azmeet): RateControl (AIMD on the worst receiver's in-flight packets)
- 54611de57 feat(azmeet): VideoOut.rate, adapt_rates() after drain_encoders, open at the adapted
  rate, AZMEET_RATE stdout line, ", H.264 at X of Y kbps" in the sending line
  => BITRATE ADAPTATION DONE (needs api.json VideoEncoder.set_bitrate).

- 5d8b51825 test(audio): Opus RED (audio/codec.rs opus_tests)
- 1a244218c feat(audio): AudioEncoder / AudioDecoder + opus_apple.rs (AudioToolbox, dlopen'd)
- 79d702934 feat(audio): wasm stubs (unified/audio.rs)
- ded162435 refactor(audio): no magic cookie (verified with a ctypes replay on this Mac: 50 packets,
  31 kbit/s, share 1.000; probe /tmp/video8/tc/opus_probe.py, not committed)
  => OPUS ENGINE DONE (needs api.json AudioEncoder / AudioDecoder).

- e9a4a428c test(azmeet): Opus wire rules RED (audio.rs opus_wire_tests, video_wire caps bit 2)
- 75f9ffa2a feat(azmeet): Opus wire rules GREEN (frame_header/read_header shared, OpusFramer,
  mix_to_mono, send_opus, caps bit)
- 6c403176f / 8df303309 OpusOrder RED / GREEN (replaced JitterBuffer::wants)
- 0fce5a9db feat(azmeet): voice as Opus when every peer decodes it (lib.rs wiring)
- 871a949a0 docs(azmeet): wire format docs
  => OPUS DONE end to end (needs api.json AudioEncoder / AudioDecoder / OptionAudioFrame).

- 8761edffa / 3c9824774 core rgba_to_nv12 RED / GREEN (RgbToYuv, the inverse of YuvCoefficients;
  test expectations replayed in /tmp/video8/tc/nv12_sim.py)
- 677a86b47 test(video): a GPU whose Vulkan driver encodes H.264 gets an open encoder (RED)
- ec5d22154 feat(video): encode_vulkan.rs + EncodeEngine (Vt | Vulkan) + encode_engine honest
- e5d869f5e docs
  => JPEG FALLBACK OFF APPLE DONE (Linux / Windows with a Vulkan Video encode GPU; not runnable here).

- f7d24e783 test(audio): EchoCanceller RED (echo_tests: converge, double talk, path change,
  passthrough, closed / other format)
- 79b3dbce2 feat(audio): EchoCanceller GREEN (FFT, two-filter MDF, Speex foreground test, stream)
- 17032ccc1 wasm stub
- 951c6d12c feat(azmeet): echo of what plays cancelled from the mic (Playout.echo, far_end per turn,
  cancel_echo in send_audio, AZMEET_ECHO_CANCEL=0)
  => ECHO CANCELLATION DONE (needs api.json EchoCanceller). Python models in /tmp/video8/tc
  (aec_proto*.py, fft_check.py) - not committed.

- zero copy steps 2-3: DESIGNED, not built (decision: touches core image model + renderer +
  cpurender + capture workers, only verifiable on a GPU window with a camera; the per-frame cost
  left is one tile-sized NV12 copy). Design in the report, section 5.
- 1e4e628f3, b759898d4, a1efd02fe report scripts/VIDEO8_2026_10_03.md

## IN PROGRESS
- nothing. TASK COMPLETE.

## NEXT
- parent: api.json entries (report "api.json" 1-6), compile, run the test commands in the report,
  then AzMeet (E2E: stdout `AZMEET_AUDIO opus`, `AZMEET_RATE ...`).

## api.json (so far) - full text goes into the report
- `audio.AudioEncoder` (external azul_dll::unified::audio::AudioEncoder; custom_impls Clone Default
  Drop; struct_fields ptr c_void mutptr, run_destructor bool; constructor create(config:
  AudioConfig, bitrate_kbps: u32); functions backend_name (static, String), is_open(ref) -> bool,
  encode(refmut, frame: AudioFrame) -> bool, recv_packet(refmut) -> OptionU8Vec, close(refmut)).
- `audio.AudioDecoder` (same shape; create(config: AudioConfig); is_open(ref) -> bool,
  decode(refmut, packet: U8Vec) -> OptionAudioFrame, close(refmut)).
- `option.OptionAudioFrame` (external azul_core::audio::OptionAudioFrame, derive Clone Debug,
  None / Some(AudioFrame), repr "C, u8").
- `audio.EchoCanceller` (external azul_dll::unified::audio::EchoCanceller; custom_impls Clone
  Default Drop; ptr c_void mutptr, run_destructor bool; constructor create(sample_rate: u32,
  tail_ms: u32); functions is_open(ref) -> bool, far_end(ref, frame: AudioFrame) -> bool,
  process(ref, frame: AudioFrame) -> AudioFrame, latency_samples(ref) -> u32, erle_db(ref) -> f32,
  close(refmut)).
- `video.VideoEncoder.functions.set_bitrate`: `fn_args: [{"self": "ref"}, {"kbps": "u32"}]`,
  returns `bool`, `fn_body: "object.set_bitrate(kbps)"`, doc "Spend `kbps` kilobits a second from
  the next frame the encoder takes on (a call whose network got slower or faster). The stream goes
  on: no new session, no forced keyframe. False when the encoder is not open."

## Decisions
- Order of the larger items by value / risk, not the backlog's listing: bitrate adaptation and
  Opus change what a call sounds and looks like on a real network; zero copy saves a ~270 KB
  memcpy + upload per tile frame now that VideoToolbox decodes NV12 at the tile's size (VIDEO_PATH),
  so it goes last.
- set_bitrate is an atomic the codec thread reads per frame, not a queued job: never blocks the
  UI thread on a full frame queue, never lost.
- The rate controller's signal is each receiver's in-flight packet count (SendWindow), the worst
  receiver decides; the uplink estimate is NOT a ceiling (spiral-down risk, see rate.rs docs).
- Pure modules are type-checked with `rustc --emit=metadata` (MEET2's precedent), test
  expectations replayed in a Python model (/tmp, not committed).

## Open questions
- (none)
