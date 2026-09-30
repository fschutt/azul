# L2_MEDIA_LIB progress

Branch `wt/l2-media-lib` from `748f999af`.

## DONE

- `387fcb813` test(media): a headless run never opens a real microphone, camera, screen or audio output (RED)
- `24dc762a3` feat(media): a headless run opens no real device, only the stand-ins it asks for

## IN PROGRESS

- 2. honest codec handles (RED)

## NEXT

- 2. fix: `VideoEncoder::open` / `VideoDecoder::open` invalid without a working engine;
  `VideoEncodeCheck` / `PlatformCapability::video_codec` report the build
- 3. iroh priorities + message backlog (RED, fix)
- report `scripts/L2_MEDIA_LIB_2026_09_29.md`

## Open questions

- none yet
