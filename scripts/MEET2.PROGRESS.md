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

- lib.rs: `a87794232` async codec drain, `1b6ea3f4a` stream_targets, `1ccb08331` no JPEG before
  caps, `002d71c37` chat, `9e41a685f` active speaker, `450b21fab` culling via arrange +
  is_node_visible. (`a34ffdefe`/`246136a89` KeyframePolicy::not_taken.)

- `c80fa4e94` RED / `e1e04689b` pace.rs; `2c4d92e57` pump paces itself, stats repaint only in
  the open overlay.

- args.rs (`f2c4f015c`/`16d6558f8`), ui.rs (`78c89d48f` skeleton, `8e93aa828` tiles, `e83b55169`
  side panel, `86d12fb3b` controls/lobby/settings, `f4bc3ad4b` key hook + facing, `5db51f356` tile
  sizing, `bb65a4029` tile ids), lib.rs swap to ui (`45aabf5ae`), callbacks (`a446d8589`), args
  wiring (`ec48019dc`), statistics people + AZMEET_PANEL (`e5fd9cf2b`).

- `c479c4f2d` scripts/azmeet_e2e.py; `ce07ec20d` chat-field layout test; `45856dda1`
  scripts/azmeet_cpu.py; `22eb8785a` RED / `690c82f3c` VideoEncoder::is_hardware; `92e7cea54`
  AzMeet reports hardware encode; `184f4847a` lobby device pickers.

- `769a5c95e` drop stage_key; report `scripts/MEET2_2026_10_01.md`.

## IN PROGRESS
- nothing: the task is done (report committed).

## NEXT (for the parent)
- api.json via autofix: VideoEncoder.flush, VideoEncoder.is_hardware, CallbackInfo.is_node_visible,
  CallShell.stage (+ set_stage / with_stage); then compile, the suites and the scripts listed in
  the report.

## Decisions
- Codec work runs on a thread per handle (CodecThread), not a shared pool: no head-of-line
  blocking between streams; the `<video>` worker and the whole-file pipeline keep their engine on
  their own thread (`open_on_this_thread`).
- Decoder output changes go through a VTPixelTransferSession stage instead of a new session at
  the next IDR.
- Lobby = CallShell with the self preview tile; statistics = a side-panel tab; settings = an
  in-window screen on ShellSettingsLayout.
- Two apps at once in the E2E / CPU scripts, each capped at 1000 MB (house-rule exception).

## Open questions / needs from LIFECYCLE
- none blocking (see the report's Coordination).
