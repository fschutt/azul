# MEET2 progress

Branch `wt/meet2` from `39092feee`. Report: `scripts/MEET2_2026_10_01.md`. Nothing is compiled
here (house rule); Rust files are parse-checked with `rustfmt --check` / `--emit stdout`.

## DONE
- `8720ac48b` RED / `d2c07bafa` fix: cut_frame keeps the worker a buffer with the frame's room.
- `aff38599a` RED / `a79c61e21` fix: avfoundation reads the YCbCr matrix by value (CFEqual).
- `bd8496003` RED / `19356b1db` fix: VideoToolbox encoder tags / range from the frames;
  decoder matrix by CFEqual; output size / format changes at the next picture (OutputStage with
  VTPixelTransferSession); CoreVideo pixel-format rule moved to capture_slot.rs (twin removed).

- `66d27172e` RED / `b8cd68319` feat: CodecThread - VideoEncoder / VideoDecoder engines on a
  thread of their own (encode queue of 3 drops when full; decode unbounded); VideoEncoder::flush
  (new API); VideoDecoder::open_on_this_thread (crate) for stream.rs / pipeline.rs.
- `477e7f539` RED / `f990d6c99` fix: the `<video>` decoder applies VideoConfig::output_format.
- `acc9d162f` RED / `9ec56f6cc` feat: NativePicture (retained IOSurface-backed decoded picture,
  copied on demand) - the zero-copy step.

- `3afed65ca` RED / `ea75727cb` feat: CallbackInfo::is_node_visible (new API) for tile culling.

- `1f48ff658` RED / `b6e144d2e` feat: CallShell `stage` (speaker layout: stage over a filmstrip).

- `cc01c483c` RED / `68aec8715` feat: chat.rs (wire kind 8, ChatLog).
- `623f02eb7` RED / `22bf62dd8` feat: speaker.rs (ActiveSpeaker, level_db).
- `b42afb3f6` RED / `6fd1ad198` feat: video_wire KeyframePolicy with OUTPUT_LAG_PACKETS,
  wire_codec (no JPEG before caps).
- `16fbdfff9` RED / `d141d27b7` feat: tiles.rs (arrange: stage / tiles; tile_need culling).

## IN PROGRESS (last commit: d141d27b7)
- next: lib.rs integration, small commits:
  a. async codec: send_h264 submits + `drain_h264` sends packets; pump drains every encoder and
     decoder (`drain_decoders`), probe_encode/probe_decode use flush.
  b. assignment_among: viewer caps unknown -> need 0 (wire_codec).
  c. chat: MeetState.chat (ChatLog), receive kind 8 in receive_item, `send_chat`.
  d. speaker: MeetState.speaker (ActiveSpeaker) fed in receive_audio (level_db of packets).
  e. culling: measure_tiles reads is_node_visible + tiles::tile_need; my_wants via arrange roles.
  f. pump: adaptive interval (15 ms with media, 250 ms idle) and no RefreshDom unless text changed.
  Then src/ui.rs (CallShell view), then scripts/azmeet_e2e.py, report.

## NEXT (plan, in order)
1. Engine video leftovers (RED then fix each):
   a. VideoToolbox: matrix by CFEqual; encoder pool / tags from the source frame (range,
      matrix); decoder output change without waiting for an IDR (VTPixelTransferSession stage).
   b. avfoundation: matrix by CFEqual.
   c. stream.rs: apply VideoConfig::output_format (one line + helper test).
   d. capture_common::cut_frame: keep the capacity when the frame travels.
   e. Codec worker: VideoEncoder / VideoDecoder run their engine on a worker thread owned by
      the handle; the UI thread only queues; `VideoEncoder::flush`.
   f. Zero-copy step (native CVPixelBuffer frames inside the dll, with a test).
   g. macOS never defaults to JPEG (AzMeet probe with the async encoder, caps before first frame).
2. AzMeet on CallShell (lobby, grid + active speaker, culled tiles, share stage, chat, people,
   controls, stats overlay, settings on ShellSettingsLayout), chat wire message.
3. scripts/azmeet_e2e.py: two peers, tiles, chat.
4. CPU: adaptive pump, no periodic RefreshDom, measure script.

## Decisions
- (filled as they are made)

## Open questions / needs from LIFECYCLE
- (filled as found)
