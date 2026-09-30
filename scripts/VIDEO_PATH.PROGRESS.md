# VIDEO_PATH progress (branch wt/video-path, base 6ae909529)

Task: AzMeet ~30% CPU in a call. Resample early, damage only the tile, NV12 end to end (GPU YUV
image, fused CPU convert), hardware low-latency VideoToolbox, screen share NV12, `--cpu` probe.
Audit folded in: scratchpad/VIDEO_AUDIT_2026_09_30.md sections (c)/(d).

## DONE
- 6dad43b2e / 8a0a01a7d  NV12 RawImageFormat (RED / fix)
- 6eeed4d18 / f788a02da  frame formats, cover crop, cascade fan-out, NV12 raster (RED / fix)
- 0622e1f66 / 3698f9396 / 8f5c3e2c7  tile damage, stable webrender keys, PaintHidden (RED x2 / fix)
- cc468b11d / 104c9c499  capture formats, slot NV12 + swap, AVFoundation / SCK (RED / fix)
- 871cee80f / 66003d063  VideoToolbox hw low-latency encoder, scaling NV12 decoder (RED / fix)
- 723a6e7e3 / 255205454  540 rung (RED / fix)
- cb795ba45  AzMeet NV12 wiring, meter 10 Hz
- 4ef66e61f  two-clients --cpu + cpu_sample.py
- ad4da92d2  docs
- report: scripts/VIDEO_PATH_2026_09_30.md

## IN PROGRESS
- none

## NEXT
- parent: api.json via autofix (list in the report), compile, run the suites, two-clients --cpu
  before/after

## Open questions
- Does VideoToolbox scale in the decompression session (Width/Height in the destination
  attributes)? Pinned by `the_decoder_hands_frames_out_at_the_asked_size` on hardware.
- Does the H.264 low-latency session take the High profile on this Mac? `settings()` logs it;
  the open falls back to hardware-required Main otherwise.
