# MEET2 progress

Branch `wt/meet2` from `39092feee`. Report: `scripts/MEET2_2026_10_01.md`. Nothing is compiled
here (house rule); Rust files are parse-checked with `rustfmt --check` / `--emit stdout`.

## DONE
- (none yet)

## IN PROGRESS
- reading: examples/azul-meet, dll video_codec (videotoolbox.rs, stream.rs, mod.rs), the reports.

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
