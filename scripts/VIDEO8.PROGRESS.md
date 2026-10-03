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

## IN PROGRESS
- bitrate adaptation

## NEXT
1. `VideoEncoder::set_bitrate(kbps)` (RED test in videotoolbox vt_tests / honest_handle_tests, then
   EncodeJob::Bitrate + VtEncoder::set_bitrate) + wasm stub.
2. AzMeet: a pure bitrate controller (RED unit tests), wired into the 2 s path-stats tick.
3. Opus through AudioToolbox on Apple (dlopen'd AudioConverter), AzMeet codec byte 2.
4. Vulkan Video H.264 encoder on Linux / Windows (encode_vulkan.rs) - no JPEG off Apple.
5. Echo cancellation (pure-Rust frequency-domain adaptive filter, synthetic-signal tests).
6. Zero copy steps 2-3 (last: lowest value now that frames decode at the tile's size in NV12).

## Decisions
- Order of the larger items by value / risk, not the backlog's listing: bitrate adaptation and
  Opus change what a call sounds and looks like on a real network; zero copy saves a ~270 KB
  memcpy + upload per tile frame now that VideoToolbox decodes NV12 at the tile's size (VIDEO_PATH),
  so it goes last.

## Open questions
- (none yet)
