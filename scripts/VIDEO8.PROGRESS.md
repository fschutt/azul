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

## IN PROGRESS
- nothing half-done; the worktree is clean after this commit.

## NEXT (exact)
1. Opus through AudioToolbox on Apple: new file dll/src/desktop/extra/audio/opus_codec.rs (or
   video_codec-style `audio_codec` module): dlopen AudioToolbox (`AudioConverterNew`,
   `AudioConverterFillComplexBuffer`, `AudioConverterDispose`), kAudioFormatOpus ('opus') 48 kHz
   mono 20 ms (960 frames); C-ABI handles `AudioEncoder` / `AudioDecoder` (ptr + run_destructor,
   `create`, `is_open`, `encode(AudioFrame) -> OptionU8Vec`, `decode(U8Vec) -> OptionAudioFrame`),
   honest closed handle off Apple; RED test: a 440 Hz tone encodes to < 200 bytes a packet and
   decodes back correlated (> 0.9). Then AzMeet audio.rs codec byte 2 (CODEC_OPUS) with PCM
   fallback when the encoder is closed or the peer's caps lack it.
2. Vulkan Video H.264 encoder on Linux / Windows: dll/src/desktop/extra/video_codec/encode_vulkan.rs
   mirroring decode_vulkan.rs (gpu-video `create_bytes_encoder_h264`, NV12 input via the core
   YCbCr table for RGBA/BGRA, RateControl::VariableBitrate; set_bitrate = re-create the encoder);
   EncoderInner gets an engine enum; encode_engine() honest.
3. Echo cancellation: pure-Rust partitioned-block frequency-domain adaptive filter (needs an FFT:
   search the tree first), synthetic-signal ERLE tests; AzMeet feeds the played far end.
4. Zero copy steps 2-3 (last).
5. Report scripts/VIDEO8_2026_10_03.md.

## api.json (so far)
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
