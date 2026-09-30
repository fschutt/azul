# VIDEO_PATH progress (branch wt/video-path, base 6ae909529)

Task: AzMeet ~30% CPU in a call. Resample early, damage only the tile, NV12 end to end (GPU YUV
image, fused CPU convert), hardware low-latency VideoToolbox, screen share NV12, `--cpu` probe.
Audit to fold in: scratchpad/VIDEO_AUDIT_2026_09_30.md sections (c)/(d) (stable keys, BGRA
premultiplied receive, preview crop not squash, encoder settings, 540 rung, SCK status/fps,
in_flight before read, vImage no Lanczos, mic meter throttle, report WR dirty rects).

## DONE
- 6dad43b2e test(core): NV12 RawImageFormat (RED)
- 8a0a01a7d feat(core): RawImageFormat NV12 x4 variants, Nv12Layout, YuvCoefficients,
  nv12_to_rgba; encoders / pdf / wr translate_image_format arms
- 6eeed4d18 test(layout): image_scale frame formats, cover crop, cascade fan-out; raster NV12 blit
  + 1:1 visible rows (RED)

- f788a02da feat(layout): VideoFrame.format, format-preserving resample_frame_rect, cover_crop,
  cascade fan_out, NV12 SrcImage, raster NV12 rows + 1:1 fast path, vImage crop/no swizzle
  (AzMeet VideoFrame literals NOT yet updated - phase C)

- 0622e1f66 test(dll): tile damage / no layout / hidden tiles (RED, headless)
- 3698f9396 test(dll): stable webrender key per tile, NV12 two planes (RED, wr_translate2)
- 8f5c3e2c7 feat: Paint no longer marks DL dirty, content_repaint_pending + headless repaint_only,
  PaintHidden gate (node_is_visible_in_window), stable node image keys + lightweight upload +
  one-time DL resend, NV12 YuvImage in compositor2, GC both planes, macOS WR dirty rects

- cc468b11d test: capture formats (RED, capture_common fake + capture_slot)
- 104c9c499 feat(capture): CaptureRequest.format, CaptureRead::FrameIn, slot NV12/packed/take_newer,
  AVFoundation '420v'/BGRA passthrough, SCK pixel format + 15 fps + frame status
- 871cee80f test(videotoolbox): NV12 in/out, scaled decode, hw settings (RED)
- 66003d063 feat(video_codec): VT hw + low-latency encoder, pooled '420v', decoder
  set_output_format/size, Vulkan NV12 passthrough, stream scaler dedup

## IN PROGRESS
- C8 AzMeet wiring (NV12 camera config, frame formats, decoder output size, JPEG path)

## NEXT
- C8 AzMeet wiring; maybe 540 rung + mic meter throttle: CaptureRead format, CaptureRequest format, CaptureSlot NV12 + swap, in_flight
  before read, AVFoundation '420v', VT encoder NV12 via pool + low-latency HW, VT decoder NV12 +
  output size (VideoDecoder::set_output_format / set_output_size), SCK '420v' + frame status + 15fps,
  AzMeet wiring (BGRA/NV12 premultiplied, 540 rung?, mic meter throttle)
- D two-clients.mjs --cpu + cpu_sample.py
- E report scripts/VIDEO_PATH_2026_09_30.md

## Open questions
- none yet
