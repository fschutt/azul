# VIDEO_PATH progress (branch wt/video-path, base 6ae909529)

Task: AzMeet ~30% CPU in a call. Resample early, damage only the tile, NV12 end to end (GPU YUV
image, fused CPU convert), hardware low-latency VideoToolbox, screen share NV12, `--cpu` probe.

## DONE
(none yet)

## IN PROGRESS
- A. NV12 format foundation (core RawImageFormat variants + helpers)

## NEXT
- A2 image_scale: format-preserving resample (BGRA stays BGRA, NV12 stays NV12), NV12 sampling,
  cascaded fan-out (each consumer cut from the smallest frame that covers it)
- A3 cpurender: NV12 blit (fused convert + scale, visible rows only), 1:1 fast path
- B1 damage only the tile: Paint tier does not mark the DL dirty, headless repaint without relayout,
  PaintHidden tier for off-screen / minimized tiles
- B2 WebRender: overlay raw images under a stable per-node key (update_image, no scene rebuild),
  NV12 as YuvImage with two keys
- C capture/codec: VideoFrame.format, CaptureRead format, CaptureSlot NV12 + swap, AVFoundation
  '420v', VT encoder NV12 via pool + low-latency HW, VT decoder NV12 + output size, SCK '420v'
  + frame status, AzMeet wiring
- D two-clients.mjs --cpu + cpu_sample.py
- E report scripts/VIDEO_PATH_2026_09_30.md

## Open questions
- none yet
